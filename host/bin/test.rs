use alloy::{
    eips::BlockId,
    providers::{Provider, ProviderBuilder},
};
use alloy_primitives::{address, b256, U256};
use alloy_trie::TrieAccount;
use anyhow::Result;
use clap::{command, Parser};
use helios_ethereum::rpc::ConsensusRpc;
use risc0_zkvm::{default_prover, ExecutorEnv};
use sp1_helios_methods::SP1_HELIOS_GUEST_ELF;
use sp1_helios_primitives::types::{ContractStorage, ProofInputs, StorageSlot};
use sp1_helios_script::{get_checkpoint, get_client, get_latest_checkpoint, get_updates};

#[derive(Parser, Debug, Clone)]
#[command(about = "Get the genesis parameters from a block.")]
pub struct GenesisArgs {
    #[arg(long)]
    pub slot: Option<u64>,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let args = GenesisArgs::parse();

    // Get the current slot from the contract or fetch the latest checkpoint
    let checkpoint = if let Some(slot) = args.slot {
        get_checkpoint(slot).await
    } else {
        get_latest_checkpoint().await
    };

    // Setup client.
    let helios_client = get_client(checkpoint).await;
    let sync_committee_updates = get_updates(&helios_client).await;
    let finality_update = helios_client.rpc.get_finality_update().await.unwrap();

    let expected_current_slot = helios_client.expected_current_slot();

    // Get the block number for the current slot
    let block_number = helios_client
        .store
        .finalized_header
        .execution()
        .unwrap()
        .block_number();

    // Expected values for the proof (mainnet Across SpokePool->crossDomainAdmin()).
    let contract_address = address!("0x5c7BCd6E7De5423a257D81B442095A1a6ced35C5");
    let storage_slot = b256!("0000000000000000000000000000000000000000000000000000000000000869");
    let expected_value = address!("c186fA914353c44b2E33eBE05f21846F1048bEda");

    // Setup execution RPC client
    let execution_rpc = std::env::var("SOURCE_EXECUTION_RPC_URL").unwrap();
    let provider = ProviderBuilder::new().on_http(execution_rpc.parse()?);

    // Get the proof using eth_getProof
    let proof = provider
        .get_proof(contract_address, vec![storage_slot])
        .block_id(BlockId::number(*block_number))
        .await?;

    let inputs = ProofInputs {
        sync_committee_updates,
        finality_update,
        expected_current_slot,
        store: helios_client.store,
        genesis_root: helios_client.config.chain.genesis_root,
        forks: helios_client.config.forks.clone(),
        contract_storage_slots: ContractStorage {
            address: contract_address,
            expected_value: TrieAccount {
                nonce: proof.nonce,
                balance: proof.balance,
                storage_root: proof.storage_hash,
                code_hash: proof.code_hash,
            },
            mpt_proof: proof.account_proof,
            storage_slots: vec![StorageSlot {
                key: storage_slot,
                expected_value: U256::from_be_slice(expected_value.as_slice()),
                mpt_proof: proof.storage_proof[0].proof.clone(),
            }],
        },
    };

    // Write the inputs to the VM
    let env = ExecutorEnv::builder()
        .write_frame(&serde_cbor::to_vec(&inputs)?)
        .build()?;

    let info = default_prover().prove(env, SP1_HELIOS_GUEST_ELF)?;
    println!("Execution Report: {:?}", info.stats);

    Ok(())
}
