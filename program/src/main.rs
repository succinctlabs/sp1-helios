//! A simple program to be proven inside the zkVM.

#![no_main]
sp1_zkvm::entrypoint!(main);

use primitives::consensus::verify_update;
use sp1_helios_primitives::types::ProofInputs;

// Cycle Tracker:
// deserialize
// └╴4,341,638 cycles
// verify_update
// ┌╴parse-generic-update
// │ └╴80,193 cycles
// │ ┌╴verify-timestamp
// │ └╴405 cycles
// │ ┌╴verify-period
// │ └╴397 cycles
// │ ┌╴verify-finality-proof
// │ └╴31,548 cycles
// │ ┌╴verify-next-committee-proof
// │ └╴3,130,323 cycles
// │ ┌╴get-sync-committee
// │ └╴388 cycles
// │ ┌╴get-participating-keys
// └╴  1,492,919,675 cycles    -- vro.
// ┌╴verify-sync-comittee-signature
// └╴387,799,352 cycles

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
