/// Generate genesis parameters for light client contract
use clap::Parser;
use helios_2_script::{
    get_checkpoint, get_client, get_execution_state_root_proof, get_latest_checkpoint,
};
use log::info;
use sp1_sdk::{HashableKey, ProverClient};
use std::env;
const TELEPATHY_ELF: &[u8] = include_bytes!("../../program/elf/riscv32im-succinct-zkvm-elf");
use alloy_primitives::B256;
use ssz_rs::prelude::*;

#[derive(Parser, Debug, Clone)]
#[command(about = "Get the genesis parameters from a block.")]
pub struct GenesisArgs {
    #[arg(long)]
    pub slot: Option<u64>,
}

#[tokio::main]
pub async fn main() {
    env::set_var("RUST_LOG", "info");
    dotenv::dotenv().ok();
    env_logger::init();

    let args = GenesisArgs::parse();

    let client = ProverClient::new();
    let (_pk, vk) = client.setup(TELEPATHY_ELF);

    let checkpoint;
    let mut verifier = String::new();
    if let Some(temp_slot) = args.slot {
        checkpoint = get_checkpoint(temp_slot).await;
    } else {
        checkpoint = get_latest_checkpoint().await;
    }
    let sp1_prover = env::var("SP1_PROVER").unwrap();
    if sp1_prover != "mock" {
        verifier = env::var("SP1_VERIFIER_ADDRESS").unwrap();
    }

    let helios_client = get_client(checkpoint.to_vec()).await;
    let finalized_header = helios_client
        .store
        .finalized_header
        .clone()
        .hash_tree_root()
        .unwrap();
    let head = helios_client.store.finalized_header.clone().slot.as_u64();
    let sync_committee_hash = helios_client
        .store
        .current_sync_committee
        .clone()
        .hash_tree_root()
        .unwrap();
    let genesis_time = helios_client.config.chain.genesis_time;
    let execution_state_root_proof = get_execution_state_root_proof(head).await.unwrap();
    let genesis_root = B256::from_slice(&helios_client.config.chain.genesis_root);
    const SECONDS_PER_SLOT: u64 = 12;
    const SLOTS_PER_EPOCH: u64 = 32;
    const SLOTS_PER_PERIOD: u64 = SLOTS_PER_EPOCH * 256;
    let source_chain_id: u64 = match env::var("SOURCE_CHAIN_ID") {
        Ok(val) => val.parse().unwrap(),
        Err(_) => {
            println!("SOURCE_CHAIN_ID not set, defaulting to mainnet");
            1 // Mainnet chain ID
        }
    };

    info!(
        "\nSP1_PROVER={}\n\
        SP1_TELEPATHY_PROGRAM_VKEY={}\n\
        SP1_VERIFIER_ADDRESS={}\n\
        CREATE2_SALT={}\n\
        GENESIS_VALIDATORS_ROOT={}\n\
        GENESIS_TIME={}\n\
        SECONDS_PER_SLOT={}\n\
        SLOTS_PER_PERIOD={}\n\
        SLOTS_PER_EPOCH={}\n\
        SOURCE_CHAIN_ID={}\n\
        SYNC_COMMITTEE_HASH={}\n\
        FINALIZED_HEADER={}\n\
        EXECUTION_STATE_ROOT={}\n\
        HEAD={}",
        sp1_prover,
        vk.bytes32(),
        verifier,
        "0xaa",
        genesis_root,
        genesis_time,
        SECONDS_PER_SLOT,
        SLOTS_PER_PERIOD,
        SLOTS_PER_EPOCH,
        source_chain_id,
        sync_committee_hash,
        finalized_header,
        execution_state_root_proof.execution_state_root,
        head
    );
}
