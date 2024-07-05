//! A simple script to generate and verify the proof of a given program.

use dotenv;
use ethers_core::types::H256;
use helios::{
    consensus::{
        constants,
        rpc::{nimbus_rpc::NimbusRpc, ConsensusRpc},
        Inner,
    },
    prelude::*,
    primitives::consensus::utils,
    primitives::types::Update,
};
use sp1_helios_primitives::types::ProofInputs;
use sp1_sdk::{utils::setup_logger, ProverClient, SP1Stdin};
use std::sync::Arc;
use tokio::sync::{mpsc::channel, watch};
use zduny_wasm_timer::SystemTime;

const ELF: &[u8] = include_bytes!("../../program/elf/riscv32im-succinct-zkvm-elf");

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

async fn get_update(client: &Inner<NimbusRpc>) -> Update {
    println!("finalized slot: {:?}", client.store.finalized_header.slot);
    let period = utils::calc_sync_period(client.store.finalized_header.slot.into());
    println!("period: {:?}", period);
    let updates = client
        .rpc
        .get_updates(period, constants::MAX_REQUEST_LIGHT_CLIENT_UPDATES)
        .await
        .unwrap();

    updates[0].clone()
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    setup_logger();

    // Generate proof.
    let mut stdin = SP1Stdin::new();

    // TODO: Read smart contract to get checkpoint / other info
    // Based on contract data, get next update and generate proof
    let checkpoint = get_latest_checkpoint().await;
    let helios_client = get_client(checkpoint.as_bytes().to_vec()).await;
    let update = get_update(&helios_client).await;
    let now = SystemTime::now();

    let inputs = ProofInputs {
        update,
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
    let mut proof = client.prove(&pk, stdin).expect("proving failed");

    // // Read output.
    let valid = proof.public_values.read::<bool>();
    println!("Is valid: {}", valid);

    // Verify proof.
    client.verify(&proof, &vk).expect("verification failed");

    // Save proof.
    proof
        .save("proof-with-io.json")
        .expect("saving proof failed");

    println!("successfully generated and verified proof for the program!")
}
