//! A simple program to be proven inside the zkVM.

#![no_main]
sp1_zkvm::entrypoint!(main);
// use helios_prover_primitives::types::{
//     BLSPubKey, Bytes32, Header, SignatureBytes, SyncAggregate, SyncCommittee, Vector, U64,
// };

use std::time::SystemTime;
use helios_prover_primitives::types::ProofInputs;
use primitives::forktypes::Forks;
use primitives::types::{Bootstrap, Bytes32, Header, LightClientStore, SyncCommittee, Update, U64};
use primitives::utils;
// use eyre::Result;

// use std::sync::Arc;

pub fn main() {
    let encoded_inputs = sp1_zkvm::io::read_vec();
    let inputs: ProofInputs = serde_cbor::from_slice(&encoded_inputs).unwrap();

    println!("{:?}", inputs);
    // let sync_committee: Update = sp1_zkvm::io::read::<Update>();
    // let update: Update = sp1_zkvm::io::read::<Update>();
    // let now: SystemTime = sp1_zkvm::io::read::<SystemTime>();
    // let genesis_time: u64 = sp1_zkvm::io::read::<u64>();
    // let store: LightClientStore = sp1_zkvm::io::read::<LightClientStore>();
    // let genesis_root: Vec<u8> = sp1_zkvm::io::read::<Vec<u8>>();
    // let forks: Forks = sp1_zkvm::io::read::<Forks>();

    println!("LFG! Program Done!");
}
