//! A simple script to generate and verify the proof of a given program.

use alloy_sol_types::{sol, SolStruct, SolType, SolValue};

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
use sp1_helios_primitives::types::{ProofInputs, ProofOutputs};
use sp1_sdk::{utils::setup_logger, ProverClient, SP1Stdin};
use std::sync::Arc;
use tokio::sync::{mpsc::channel, watch};
use tracing::{debug, error, info, warn};
use zduny_wasm_timer::SystemTime;
const ELF: &[u8] = include_bytes!("../../program/elf/riscv32im-succinct-zkvm-elf");
use ssz_rs::prelude::*;

async fn get_latest_checkpoint() -> H256 {
    let cf = checkpoints::CheckpointFallback::new()
        .build()
        .await
        .unwrap();

    // Fetch the latest mainnet checkpoint
    let mainnet_checkpoint = cf
        .fetch_latest_checkpoint(&networks::Network::MAINNET)
        .await
        .unwrap();
    println!(
        "Fetched latest mainnet checkpoint: {:?}",
        mainnet_checkpoint
    );

    mainnet_checkpoint
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

async fn get_updates(client: &Inner<NimbusRpc>) -> Vec<Update> {
    println!("finalized slot: {:?}", client.store.finalized_header.slot);
    let period = utils::calc_sync_period(client.store.finalized_header.slot.into());
    println!("period: {:?}", period);
    let updates = client
        .rpc
        .get_updates(period, constants::MAX_REQUEST_LIGHT_CLIENT_UPDATES)
        .await
        .unwrap();

    updates.clone()
}

async fn get_checkpoint_for_epoch(epoch: u64) -> H256 {
    let rpc = NimbusRpc::new("https://www.lightclientdata.org");
    const SLOTS_PER_EPOCH: u64 = 32;

    let first_slot = epoch * SLOTS_PER_EPOCH;
    let mut block = rpc.get_block(first_slot).await.unwrap();
    H256::from_slice(block.hash_tree_root().unwrap().as_ref())
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    setup_logger();

    // Generate proof.
    let mut stdin = SP1Stdin::new();

    // TODO: Read smart contract to get checkpoint / other info
    // Based on contract data, get next update and generate proof

    let checkpoint = get_checkpoint_for_epoch(295000).await;
    let helios_client = get_client(checkpoint.as_bytes().to_vec()).await;

    let updates = get_updates(&helios_client).await;
    let now = SystemTime::now();
    let finality_update = helios_client.rpc.get_finality_update().await.unwrap();

    let inputs = ProofInputs {
        updates,
        finality_update,
        now,
        genesis_time: helios_client.config.chain.genesis_time,
        store: helios_client.store,
        genesis_root: helios_client.config.chain.genesis_root.clone(),
        forks: helios_client.config.forks.clone(),
    };
    let encoded_inputs = serde_cbor::to_vec(&inputs).unwrap();
    stdin.write_slice(&encoded_inputs);

    let client = ProverClient::new();
    let (pk, vk) = client.setup(ELF);
    // let (_, report) = client.execute(ELF, stdin).expect("execution failed");
    // println!("{:?}", report);
    let mut proof = client.prove_plonk(&pk, stdin).expect("proving failed");

    // Read output.
    let public_values = proof.public_values.as_ref();
    let proof_outputs = ProofOutputs::abi_decode(public_values, true).unwrap();

    println!("{:?}", proof_outputs);

    // Verify proof.
    client
        .verify_plonk(&proof, &vk)
        .expect("verification failed");

    // Save proof.
    proof
        .save("proof-with-io.json")
        .expect("saving proof failed");

    println!("successfully generated and verified proof for the program!");

    info!(
    target: "helios::consensus",
        "consensus client in sync with checkpoint: 0x{}",
        hex::encode(proof_outputs.1)
    );
}
