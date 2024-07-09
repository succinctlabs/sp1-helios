//! A simple program to be proven inside the zkVM.

#![no_main]
sp1_zkvm::entrypoint!(main);

use alloy_primitives::{B256, U256};
use alloy_sol_types::{sol, SolType};
use common::consensus::{
    apply_finality_update, apply_update, verify_finality_update, verify_update,
};
use sp1_helios_primitives::types::{ProofInputs, ProofOutputs};
use ssz_rs::prelude::*;

pub fn main() {
    let encoded_inputs = sp1_zkvm::io::read_vec();

    println!("cycle-tracker-start: deserialize");
    let ProofInputs {
        updates,
        finality_update,
        now,
        genesis_time,
        mut store,
        genesis_root,
        forks,
    } = serde_cbor::from_slice(&encoded_inputs).unwrap();
    println!("cycle-tracker-end: deserialize");

    let mut is_valid = true;
    let prev_header: B256 = store
        .finalized_header
        .hash_tree_root()
        .unwrap()
        .as_ref()
        .try_into()
        .unwrap();
    let prev_sync_committee_hash: B256 = store
        .current_sync_committee
        .hash_tree_root()
        .unwrap()
        .as_ref()
        .try_into()
        .unwrap();
    let prev_head = store.finalized_header.slot;

    println!("cycle-tracker-start: verify_and_apply_update");
    for (index, update) in updates.iter().enumerate() {
        println!("Processing update {} of {}", index + 1, updates.len());

        println!("cycle-tracker-start: verify_update");
        is_valid = is_valid
            && verify_update(
                update,
                now,
                genesis_time,
                &store,
                genesis_root.clone(),
                &forks,
            )
            .is_ok();
        println!("cycle-tracker-end: verify_update");

        println!("cycle-tracker-start: apply_update");
        apply_update(&mut store, update);
        println!("cycle-tracker-end: apply_update");
    }

    is_valid = is_valid
        && verify_finality_update(
            &finality_update,
            now,
            genesis_time,
            &store,
            genesis_root.clone(),
            &forks,
        )
        .is_ok();
    apply_finality_update(&mut store, &finality_update);

    println!("cycle-tracker-end: verify_and_apply_update");

    assert!(is_valid);

    let header: B256 = store
        .finalized_header
        .hash_tree_root()
        .unwrap()
        .as_ref()
        .try_into()
        .unwrap();
    let sync_committee_hash: B256 = store
        .current_sync_committee
        .hash_tree_root()
        .unwrap()
        .as_ref()
        .try_into()
        .unwrap();
    let head = store.finalized_header.slot;

    let proof_outputs = ProofOutputs::abi_encode(&(
        prev_header,
        header,
        prev_sync_committee_hash,
        sync_committee_hash,
        U256::from(prev_head.as_u64()),
        U256::from(head.as_u64()),
    ));
    sp1_zkvm::io::commit_slice(&proof_outputs);
}
