//! A simple script to generate and verify the proof of a given program.

use dotenv;
use ethers::types::H256;
use eyre::Result;
use helios::{
    client,
    consensus::{
        self, constants,
        rpc::{nimbus_rpc::NimbusRpc, ConsensusRpc},
        types::{Bootstrap, Update},
        utils, Inner,
    },
    prelude::*,
};
use primitives::fetcher::{
    get_bootstrap, get_client, get_latest_checkpoint, get_update, to_branch, to_committee,
    to_header, to_sync_agg,
};
use primitives::types::{
    BLSPubKey, Bytes32, Header, SignatureBytes, SyncAggregate, SyncCommittee, Vector, U64,
};

use sp1_sdk::{ProverClient, SP1Stdin};
use ssz_rs::Serialize;
use std::sync::Arc;
use tokio::sync::{mpsc::channel, watch};
const ELF: &[u8] = include_bytes!("../../program/elf/riscv32im-succinct-zkvm-elf");
use primitives::consensus::verify_update;

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();

    // Generate proof.
    let mut stdin = SP1Stdin::new();

    let checkpoint = get_latest_checkpoint().await;
    let helios_client = get_client(checkpoint.as_bytes().to_vec()).await;
    // let bootstrap = get_bootstrap(&helios_client, checkpoint.as_bytes()).await;
    let update = get_update(&helios_client).await;
    verify_update(
        &update,
        helios_client.store,
        helios_client.config,
        SystemTime::now(),
    );

    // stdin.write(&buf);

    // let client = ProverClient::new();
    // let (pk, vk) = client.setup(ELF);
    // let mut proof = client.prove(&pk, stdin).expect("proving failed");

    // // Read output.
    // let valid = proof.public_values.read::<bool>();
    // println!("valid: {}", valid);

    // Verify proof.
    // client.verify(&proof, &vk).expect("verification failed");

    // // Save proof.
    // proof
    //     .save("proof-with-io.json")
    //     .expect("saving proof failed");

    // println!("successfully generated and verified proof for the program!")
}
