#![no_main]
sp1_zkvm::entrypoint!(main);

use alloy_primitives::B256;
use alloy_sol_types::SolValue;
use sp1_helios_primitives::{
    types::{ContractStorage, StorageProofOutputs},
    verify_storage_slot_proofs,
};

pub fn main() {
    let storage: Vec<ContractStorage> = sp1_zkvm::io::read();
    let state_root: B256 = sp1_zkvm::io::read();

    let storage_slots = storage
        .iter()
        .flat_map(|contract_storage| {
            verify_storage_slot_proofs(state_root, contract_storage)
                .expect("Storage slot proofs failed to verify.")
        })
        .collect();

    let proof_outputs = StorageProofOutputs {
        stateRoot: state_root,
        storageSlots: storage_slots,
    };

    sp1_zkvm::io::commit_slice(&proof_outputs.abi_encode());
}
