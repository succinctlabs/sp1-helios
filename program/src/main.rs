//! A simple program to be proven inside the zkVM.

#![no_main]
sp1_zkvm::entrypoint!(main);

use common::consensus::{
    apply_finality_update, apply_update, verify_finality_update, verify_update,
};
use sp1_helios_primitives::types::ProofInputs;

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
    println!("Num updates: {}", updates.len());
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

    sp1_zkvm::io::commit(&is_valid);
    sp1_zkvm::io::commit(&store);
}
