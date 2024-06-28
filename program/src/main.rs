//! A simple program to be proven inside the zkVM.

#![no_main]
sp1_zkvm::entrypoint!(main);
// use helios_prover_primitives::types::{
//     BLSPubKey, Bytes32, Header, SignatureBytes, SyncAggregate, SyncCommittee, Vector, U64,
// };

use eyre::Result;

use std::sync::Arc;

pub fn main() {
    // let checkpoint: H256 = sp1_zkvm::io::read::<H256>();
    // let mut bootstrap: Bootstrap = sp1_zkvm::io::read::<Bootstrap>();

    // let client = get_client(checkpoint.as_bytes().to_vec(), &mut bootstrap);
    println!("LFG! Program Done!");
}
