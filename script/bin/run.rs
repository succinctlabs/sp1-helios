use alloy_primitives::B256;
use ethers::prelude::*;
/// Update the light client once
use helios::consensus::rpc::ConsensusRpc;
use helios_script::{get_execution_state_root_proof, get_updates};
use sp1_helios_primitives::types::ProofInputs;
use sp1_sdk::{utils::setup_logger, ProverClient, SP1Stdin};
use std::sync::Arc;
use tracing::{error, info};

const ELF: &[u8] = include_bytes!("../../program/elf/riscv32im-succinct-zkvm-elf");

use anyhow::Result;
use helios_script::*;
use std::env;

abigen!(
    SP1LightClient,
    r#"[
        function update(bytes calldata proof, bytes calldata publicValues) external
        function head() external view returns (uint256)
        function getSyncCommitteePeriod(uint256 slot) external view returns (uint256)
        function syncCommittees(uint256 period) external view returns (bytes32)
    ]"#
);

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    setup_logger();
    let chain_id: u64 = env::var("CHAIN_ID")
        .expect("CHAIN_ID not set")
        .parse()
        .unwrap();
    let rpc_url = env::var("RPC_URL").expect("RPC_URL not set");
    let private_key = env::var("PRIVATE_KEY").expect("PRIVATE_KEY not set");
    let contract_address: Address = env::var("CONTRACT_ADDRESS")
        .expect("CONTRACT_ADDRESS not set")
        .parse()
        .unwrap();

    let wallet: LocalWallet = private_key.parse().expect("Failed to parse private key");
    let provider = Provider::<Http>::try_from(rpc_url).unwrap();
    let client = SignerMiddleware::new(provider.clone(), wallet.clone());
    let contract = SP1LightClient::new(contract_address, Arc::new(client));

    let head: u64 = contract.head().call().await?.as_u64();
    let period: u64 = contract
        .get_sync_committee_period(head.into())
        .call()
        .await?
        .as_u64();
    let contract_current_sync_committee = contract.sync_committees(period.into()).call().await?;
    let contract_next_sync_committee = contract.sync_committees((period + 1).into()).call().await?;

    // Fetch the checkpoint at that slot
    let checkpoint = get_checkpoint(head).await;

    // Setup & sync client.
    let  helios_client = get_client(checkpoint.as_bytes().to_vec()).await;
    let updates = get_updates(&helios_client).await;

    let (helios_client, updates) = sync_client(
        helios_client,
        updates,
        head,
        B256::from(contract_current_sync_committee),
        B256::from(contract_next_sync_committee),
    )
    .await;

    let finality_update = helios_client.rpc.get_finality_update().await.unwrap();
    let latest_block = finality_update.finalized_header.slot;

    // if latest_block.as_u64() <= head {
    //     info!("Contract is up to date. Nothing to update.");
    //     return Ok(());
    // }

    let execution_state_root_proof = get_execution_state_root_proof(latest_block.into())
        .await
        .unwrap();

    let expected_current_slot = helios_client.expected_current_slot();
    let inputs = ProofInputs {
        updates,
        finality_update,
        expected_current_slot,
        store: helios_client.store.clone(),
        genesis_root: helios_client
            .config
            .chain
            .genesis_root
            .clone()
            .try_into()
            .unwrap(),
        forks: helios_client.config.forks.clone(),
        execution_state_proof: execution_state_root_proof,
    };

    let mut stdin = SP1Stdin::new();
    let encoded_inputs = serde_cbor::to_vec(&inputs).unwrap();
    stdin.write_slice(&encoded_inputs);

    // Generate proof.
    let client = ProverClient::new();
    let (pk, _) = client.setup(ELF);
    let proof = client.prove(&pk, stdin).plonk().run().unwrap();

    // Relay the update to the contract
    let proof_as_bytes = if env::var("SP1_PROVER").unwrap().to_lowercase() == "mock" {
        vec![]
    } else {
        proof.bytes()
    };
    let public_values_bytes = proof.public_values.to_vec();

    let gas_limit = relay::get_gas_limit(chain_id);
    let max_fee_per_gas = relay::get_fee_cap(chain_id, &provider).await;

    let nonce = provider
        .get_transaction_count(wallet.address(), None)
        .await?;

    const NUM_CONFIRMATIONS: usize = 3;

    let tx = contract
        .update(proof_as_bytes.into(), public_values_bytes.into())
        .gas(gas_limit)
        .gas_price(max_fee_per_gas)
        .nonce(nonce);

    let pending_tx = tx.send().await?;
    let receipt = pending_tx
        .confirmations(NUM_CONFIRMATIONS)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Transaction failed to confirm"))?;

    if receipt.status != Some(1.into()) {
        error!("Transaction reverted!");
    } else {
        info!("Transaction hash: {:?}", receipt.transaction_hash);
    }

    Ok(())
}
