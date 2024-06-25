//! A simple script to generate and verify the proof of a given program.

use dotenv;
use ethers::types::H256;
use eyre::Result;
use helios::{
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
use std::sync::Arc;
use tokio::sync::{mpsc::channel, watch};

const ELF: &[u8] = include_bytes!("../../program/elf/riscv32im-succinct-zkvm-elf");

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();

    // Generate proof.
    let mut stdin = SP1Stdin::new();

    let checkpoint = get_latest_checkpoint().await;
    let client = get_client(checkpoint.as_bytes().to_vec()).await;
    let bootstrap = get_bootstrap(&client, checkpoint.as_bytes()).await;
    let update = get_update(&client).await;

    let attested_header = to_header(update.attested_header.clone());

    let finality_branch = to_branch(update.finality_branch);

    let finalized_header = to_header(update.finalized_header);
    let current_sync_committee = to_committee(bootstrap.current_sync_committee);
    let current_sync_committee_branch = to_branch(bootstrap.current_sync_committee_branch);
    let next_committee = to_committee(update.next_sync_committee);
    let next_sync_committee_branch = to_branch(update.next_sync_committee_branch);
    let sync_aggregate = to_sync_agg(update.sync_aggregate);

    stdin.write(&attested_header);
    stdin.write(&finality_branch);
    stdin.write(&finalized_header);
    stdin.write(&current_sync_committee);
    stdin.write(&current_sync_committee_branch);
    stdin.write(&next_committee);
    stdin.write(&next_sync_committee_branch);
    stdin.write(&sync_aggregate);

    let client = ProverClient::new();
    let (pk, vk) = client.setup(ELF);
    let mut proof = client.prove(&pk, stdin).expect("proving failed");

    // Read output.
    let valid = proof.public_values.read::<bool>();
    println!("valid: {}", valid);

    // Verify proof.
    client.verify(&proof, &vk).expect("verification failed");

    // Save proof.
    proof
        .save("proof-with-io.json")
        .expect("saving proof failed");

    println!("successfully generated and verified proof for the program!")
}
