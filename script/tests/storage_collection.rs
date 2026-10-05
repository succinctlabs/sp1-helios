//! Reproduce wrong-chain and stale-block collection using real local Ethereum trie proofs.
//!
//! Anvil supplies non-empty proofs on two chains. The legacy collection methods retain their
//! original bodies; the corrected source RPC and committed storage ELF are exercised directly.

use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use alloy::primitives::{address, B256, U256};
use alloy::providers::ProviderBuilder;
use alloy::sol_types::SolValue;
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use sp1_helios_primitives::{
    types::{ContractStorage, ProofInputs, StorageProofOutputs},
    verify_storage_slot_proofs,
};
use sp1_helios_script::execution::{fill_execution_header, ExecutionRpc};
use sp1_sdk::{Elf, Prover, ProverClient, SP1Stdin};

#[path = "support/legacy_storage_collector.rs"]
mod legacy;

const STORAGE_ELF: &[u8] = include_bytes!("../../elf/storage");
const ADDRESS: alloy::primitives::Address = address!("0000000000000000000000000000000000001000");
const KEY: B256 = B256::ZERO;

struct LocalChain {
    child: Child,
    url: String,
    client: reqwest::Client,
}

impl Drop for LocalChain {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl LocalChain {
    async fn start(chain_id: u64) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        drop(listener);
        let child = Command::new("anvil")
            .args([
                "--host",
                "127.0.0.1",
                "--port",
                &port.to_string(),
                "--chain-id",
                &chain_id.to_string(),
                "--no-mining",
                "--silent",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .context("Install Foundry to run the local-chain storage regressions")?;
        let mut chain = Self {
            child,
            url: format!("http://127.0.0.1:{port}"),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(2))
                .build()?,
        };
        for _ in 0..100 {
            if chain.rpc("eth_chainId", json!([])).await.is_ok() {
                return Ok(chain);
            }
            ensure!(
                chain.child.try_wait()?.is_none(),
                "Anvil exited before startup"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        anyhow::bail!("Anvil did not start")
    }

    async fn rpc(&self, method: &str, params: Value) -> Result<Value> {
        let response: Value = self
            .client
            .post(&self.url)
            .json(&json!({"jsonrpc":"2.0", "id":1, "method":method, "params":params}))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        ensure!(response.get("error").is_none(), "{method}: {response}");
        response
            .get("result")
            .cloned()
            .context("Missing RPC result")
    }

    async fn set_and_mine(&self, value: u64) -> Result<u64> {
        if self.rpc("eth_getCode", json!([ADDRESS, "latest"])).await? == "0x" {
            // The contract stores the calldata word in slot zero. Mine real transactions so
            // historical proofs retain their original state instead of mutating Anvil's DB.
            self.rpc("anvil_setCode", json!([ADDRESS, "0x60003560005500"]))
                .await?;
        }
        let accounts = self.rpc("eth_accounts", json!([])).await?;
        let transaction = self
            .rpc(
                "eth_sendTransaction",
                json!([{
                    "from": accounts[0],
                    "to": ADDRESS,
                    "data": B256::from(U256::from(value).to_be_bytes::<32>()),
                    "gas": "0x186a0",
                    "gasPrice": "0x3b9aca00"
                }]),
            )
            .await?;
        self.rpc("evm_mine", json!([])).await?;
        let receipt = self
            .rpc("eth_getTransactionReceipt", json!([transaction]))
            .await?;
        ensure!(
            receipt["status"] == "0x1",
            "Storage-setting transaction failed: {receipt}"
        );
        let block = self.rpc("eth_blockNumber", json!([])).await?;
        Ok(u64::from_str_radix(
            block
                .as_str()
                .context("Invalid block number")?
                .trim_start_matches("0x"),
            16,
        )?)
    }
}

fn storage_stdin(proofs: &[ContractStorage], root: B256) -> SP1Stdin {
    let mut stdin = SP1Stdin::new();
    stdin.write(&proofs);
    stdin.write(&root);
    stdin
}

#[tokio::test]
async fn collection_regressions_reject_wrong_chain_and_stale_block() -> Result<()> {
    let source = LocalChain::start(31337).await?;
    let destination = LocalChain::start(31338).await?;
    let starting_block = source.set_and_mine(10).await?;
    let destination_block = destination.set_and_mine(999).await?;
    assert_eq!(starting_block, destination_block);
    let target_block = source.set_and_mine(20).await?;

    let source_rpc = ExecutionRpc::new(&source.url)?;
    source_rpc.check_chain_id(31337).await?;
    let destination_rpc = ExecutionRpc::new(&destination.url)?;
    assert!(destination_rpc.check_chain_id(31337).await.is_err());
    let starting_header = source_rpc.header_by_number(starting_block).await?;
    let target_header = source_rpc.header_by_number(target_block).await?;
    assert_ne!(starting_header.state_root, target_header.state_root);

    let destination_header = destination_rpc.header_by_number(destination_block).await?;
    destination_rpc
        .storage_proof(
            destination_header.state_root,
            destination_block,
            ADDRESS,
            vec![KEY],
        )
        .await
        .context("Positive control: destination proof must match its own root")?;
    source_rpc
        .storage_proof(
            starting_header.state_root,
            starting_block,
            ADDRESS,
            vec![KEY],
        )
        .await
        .context("Positive control: historical source proof must match its own root")?;

    // A valid target proof is the positive control for both native and guest verification.
    let correct = source_rpc
        .storage_proof(target_header.state_root, target_block, ADDRESS, vec![KEY])
        .await?;
    let slots = verify_storage_slot_proofs(target_header.state_root, &correct)?;
    assert_eq!(slots.len(), 1);
    assert_eq!(
        slots[0].value,
        B256::from(U256::from(20).to_be_bytes::<32>())
    );

    let old_destination_provider = ProviderBuilder::new().connect_http(destination.url.parse()?);
    let wrong_chain = legacy::LegacyCollector::new(old_destination_provider, ADDRESS, KEY)
        .collect(starting_block)
        .await?;
    assert_eq!(wrong_chain[0].storage_slots[0].value, U256::from(999));
    assert!(verify_storage_slot_proofs(target_header.state_root, &wrong_chain[0]).is_err());

    // Use the source provider to isolate stale-block selection from wrong-chain selection.
    let old_source_provider = ProviderBuilder::new().connect_http(source.url.parse()?);
    let stale = legacy::LegacyCollector::new(old_source_provider, ADDRESS, KEY)
        .collect(starting_block)
        .await?;
    assert_eq!(stale[0].storage_slots[0].value, U256::from(10));
    assert!(verify_storage_slot_proofs(target_header.state_root, &stale[0]).is_err());

    // The corrected fetch fails early if either an old block or another chain is selected.
    assert!(source_rpc
        .storage_proof(target_header.state_root, starting_block, ADDRESS, vec![KEY])
        .await
        .is_err());
    assert!(destination_rpc
        .storage_proof(
            target_header.state_root,
            destination_block,
            ADDRESS,
            vec![KEY]
        )
        .await
        .is_err());

    let client = ProverClient::builder().cpu().build().await;
    let correct = vec![correct];
    let (values, report) = client
        .execute(
            Elf::Static(STORAGE_ELF),
            storage_stdin(&correct, target_header.state_root),
        )
        .await?;
    assert_eq!(report.exit_code, 0);
    let outputs = StorageProofOutputs::abi_decode(values.as_slice())?;
    assert_eq!(outputs.stateRoot, target_header.state_root);
    assert_eq!(outputs.storageSlots.len(), 1);
    assert_eq!(outputs.storageSlots[0].value, slots[0].value);

    for (label, invalid) in [("wrong chain", wrong_chain), ("stale block", stale)] {
        let (values, report) = client
            .execute(
                Elf::Static(STORAGE_ELF),
                storage_stdin(&invalid, target_header.state_root),
            )
            .await?;
        // SDK execution returns a report even when the guest panics. A returned Result alone
        // does not establish successful execution or a committed public output.
        assert_ne!(
            report.exit_code, 0,
            "Mismatched proof must fail guest execution"
        );
        assert!(
            values.as_slice().is_empty(),
            "Failed guest must not commit storage values"
        );
        println!(
            "{label}: storage guest exited with code {} and no public values",
            report.exit_code
        );
    }
    println!(
        "target value 20 accepted; legacy destination value 999 and starting value 10 rejected"
    );
    Ok(())
}

#[tokio::test]
async fn update_collection_uses_the_resulting_execution_block() -> Result<()> {
    let mut inputs: ProofInputs =
        serde_cbor::from_slice(include_bytes!("fixtures/proof_inputs.cbor"))?;
    let starting = inputs.store.finalized_header.execution().unwrap();
    let old_block = *starting.block_number();
    let old_root = *starting.state_root();
    assert!(inputs.contract_storage.is_empty());

    // This real consensus fixture embeds its execution header, so no execution RPC is needed.
    let rpc = ExecutionRpc::new("http://127.0.0.1:0")?;
    let (_, selected) = fill_execution_header(&mut inputs, &rpc).await?;
    assert!(selected.block_number > old_block);
    assert_ne!(selected.state_root, old_root);
    assert_eq!(
        *inputs
            .store
            .finalized_header
            .execution()
            .unwrap()
            .block_number(),
        old_block
    );
    println!(
        "old collector selected execution block {old_block}; corrected preview selected {}",
        selected.block_number
    );
    Ok(())
}
