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
use primitives::types::{
    BLSPubKey, Bytes32, Header, SignatureBytes, SyncAggregate, SyncCommittee, Vector, U64,
};
use sp1_sdk::{ProverClient, SP1Stdin};
use std::sync::Arc;
use tokio::sync::{mpsc::channel, watch};


const ELF: &[u8] = include_bytes!("../../program/elf/riscv32im-succinct-zkvm-elf");

fn main() {
    // Setup logging.
    dotenv::dotenv().ok();

    // Generate proof.
    let mut stdin = SP1Stdin::new();
    let n = 186u32;
    stdin.write(&n);
    let client = ProverClient::new();
    let (pk, vk) = client.setup(ELF);
    let mut proof = client.prove(&pk, stdin).expect("proving failed");

    // Read output.
    let a = proof.public_values.read::<u128>();
    let b = proof.public_values.read::<u128>();
    println!("a: {}", a);
    println!("b: {}", b);

    // Verify proof.
    client.verify(&proof, &vk).expect("verification failed");

    // Save proof.
    proof
        .save("proof-with-io.json")
        .expect("saving proof failed");

    println!("successfully generated and verified proof for the program!")
}
