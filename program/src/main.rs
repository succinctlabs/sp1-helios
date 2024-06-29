//! A simple program to be proven inside the zkVM.

#![no_main]
sp1_zkvm::entrypoint!(main);
// use helios_prover_primitives::types::{
//     BLSPubKey, Bytes32, Header, SignatureBytes, SyncAggregate, SyncCommittee, Vector, U64,
// };

use helios_prover_primitives::types::ProofInputs;
use primitives::consensus::verify_update;
use primitives::forktypes::Forks;
use primitives::types::{Bootstrap, Bytes32, Header, LightClientStore, SyncCommittee, Update, U64};
use primitives::utils;
use std::time::SystemTime;
// use eyre::Result;

// use std::sync::Arc;

pub fn main() {
    let encoded_inputs = sp1_zkvm::io::read_vec();

    println!("cycle-tracker-start: deserialize");
    let ProofInputs {
        update,
        now,
        genesis_time,
        store,
        genesis_root,
        forks,
    } = serde_cbor::from_slice(&encoded_inputs).unwrap();
    println!("cycle-tracker-end: deserialize");

    println!("cycle-tracker-start: verify_update");
    let is_valid = verify_update(&update, now, genesis_time, store, genesis_root, &forks).is_ok();
    println!("cycle-tracker-end: verify_update");

    assert!(is_valid);

    sp1_zkvm::io::commit(&is_valid);
}
