//! A simple program to be proven inside the zkVM.

#![no_main]
sp1_zkvm::entrypoint!(main);

use common::consensus::verify_update;
use sp1_helios_primitives::types::ProofInputs;

pub fn main() {
    let encoded_inputs = sp1_zkvm::io::read_vec();

    println!("cycle-tracker-start: deserialize");
    let ProofInputs {
        updates,
        now,
        genesis_time,
        mut store,
        genesis_root,
        forks,
    } = serde_cbor::from_slice(&encoded_inputs).unwrap();
    println!("cycle-tracker-end: deserialize");

    println!("cycle-tracker-start: verify_update");
    let mut is_valid = true;
    for update in updates {
        is_valid = is_valid
            && verify_update(
                &update,
                now,
                genesis_time,
                &store,
                genesis_root.clone(),
                &forks,
            )
            .is_ok();
        // apply_update(bla)
    }

    println!("cycle-tracker-end: verify_update");

    assert!(is_valid);

    sp1_zkvm::io::commit(&is_valid);
}
