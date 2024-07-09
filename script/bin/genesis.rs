//! To build the binary:
//!
//!     `cargo build --release --bin genesis`
//!
//!
//!
//!
//!

use clap::Parser;
use log::info;
use sp1_sdk::{HashableKey, ProverClient};
use std::env;
const TELEPATHY_ELF: &[u8] = include_bytes!("../../program/elf/riscv32im-succinct-zkvm-elf");
use alloy_primitives::B256;
use ethers_core::types::H256;
use helios::{
    common::consensus::types::Update,
    common::consensus::utils,
    consensus::{
        constants,
        rpc::{nimbus_rpc::NimbusRpc, ConsensusRpc},
        Inner,
    },
    prelude::*,
};
use ssz_rs::prelude::*;
use std::sync::Arc;
use tokio::sync::{mpsc::channel, watch};

#[derive(Parser, Debug, Clone)]
#[command(about = "Get the genesis parameters from a block.")]
pub struct GenesisArgs {
    #[arg(long)]
    pub epoch: Option<u64>,
    pub verifier: Option<String>,
}

async fn get_latest_checkpoint() -> H256 {
    let cf = checkpoints::CheckpointFallback::new()
        .build()
        .await
        .unwrap();

    // Fetch the latest mainnet checkpoint

    cf.fetch_latest_checkpoint(&networks::Network::MAINNET)
        .await
        .unwrap()
}

async fn get_client(checkpoint: Vec<u8>) -> Inner<NimbusRpc> {
    let consensus_rpc = "https://www.lightclientdata.org";

    let base_config = networks::mainnet();
    let config = Config {
        consensus_rpc: consensus_rpc.to_string(),
        execution_rpc: String::new(),
        chain: base_config.chain,
        forks: base_config.forks,
        strict_checkpoint_age: false,
        ..Default::default()
    };

    let (block_send, _) = channel(256);
    let (finalized_block_send, _) = watch::channel(None);
    let (channel_send, _) = watch::channel(None);

    let mut client = Inner::<NimbusRpc>::new(
        consensus_rpc,
        block_send,
        finalized_block_send,
        channel_send,
        Arc::new(config),
    );

    client.bootstrap(&checkpoint).await.unwrap();
    client
}

async fn get_checkpoint_for_epoch(epoch: u64) -> H256 {
    let rpc = NimbusRpc::new("https://www.lightclientdata.org");
    const SLOTS_PER_EPOCH: u64 = 32;

    let first_slot = epoch * SLOTS_PER_EPOCH;
    let mut block = rpc.get_block(first_slot).await.unwrap();
    H256::from_slice(block.hash_tree_root().unwrap().as_ref())
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
    let verifier;
    let sp1_prover;
    if let Some(temp_epoch) = args.epoch {
        checkpoint = get_checkpoint_for_epoch(temp_epoch).await;
    } else {
        checkpoint = get_latest_checkpoint().await;
    }
    if let Some(temp_verifier) = args.verifier {
        verifier = temp_verifier;
        sp1_prover = String::new();
    } else {
        verifier = String::new();
        sp1_prover = "mock".to_string()
    }

    let helios_client = get_client(checkpoint.as_bytes().to_vec()).await;
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
    let genesis_root = B256::from_slice(&helios_client.config.chain.genesis_root);
    const SECONDS_PER_SLOT: u64 = 12;
    const SLOTS_PER_EPOCH: u64 = 32;
    const SLOTS_PER_PERIOD: u64 = SLOTS_PER_EPOCH * 256;

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
        SYNC_COMMITTEE_HASH={}\n\
        FINALIZED_HEADER={}\n\
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
        sync_committee_hash,
        finalized_header,
        head
    );
}
