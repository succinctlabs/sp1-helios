#![no_main]
sp1_zkvm::entrypoint!(main);

use alloy_primitives::{B256, U256};
use alloy_sol_types::SolValue;
use helios_consensus_core::{
    apply_finality_update, apply_update, verify_finality_update, verify_update,
};
use sp1_helios_primitives::{
    types::{ExecutionHeaderProofOutputs, ProofInputs},
    verify_storage_slot_proofs,
};
use tree_hash::TreeHash;

/// Verifies a Helios finality update and commits finalized execution header fields needed by
/// consumers that prove receipt/log inclusion against a finalized execution block.
pub fn main() {
    let encoded_inputs = sp1_zkvm::io::read_vec();

    let ProofInputs {
        updates,
        finality_update,
        expected_current_slot,
        mut store,
        genesis_root,
        forks,
        contract_storage,
    } = serde_cbor::from_slice(&encoded_inputs).unwrap();

    // SECURITY: `store` is prover-controlled input. Keep this in sync with the legacy light-client
    // program so V2 does not reopen GHSA-83q5-vwj7-gxww.
    store.next_sync_committee = None;

    let prev_sync_committee_hash = store.current_sync_committee.tree_hash_root();
    let prev_header: B256 = store.finalized_header.beacon().tree_hash_root();
    let prev_head = store.finalized_header.beacon().slot;

    for (index, update) in updates.iter().enumerate() {
        println!("Verifying update {} of {}.", index + 1, updates.len());
        verify_update(update, expected_current_slot, &store, genesis_root, &forks)
            .expect("Update is invalid!");
        apply_update(&mut store, update);
    }

    verify_finality_update(
        &finality_update,
        expected_current_slot,
        &store,
        genesis_root,
        &forks,
    )
    .expect("Finality update failed to verify.");

    apply_finality_update(&mut store, &finality_update);

    assert!(
        store.finalized_header.beacon().slot > prev_head,
        "New head is not greater than previous head."
    );
    assert!(
        store.finalized_header.beacon().slot.is_multiple_of(32),
        "New head is not a checkpoint slot."
    );

    let header: B256 = store.finalized_header.beacon().tree_hash_root();
    let sync_committee_hash: B256 = store.current_sync_committee.tree_hash_root();
    let next_sync_committee_hash: B256 = match &mut store.next_sync_committee {
        Some(next_sync_committee) => next_sync_committee.tree_hash_root(),
        None => B256::ZERO,
    };
    let head = store.finalized_header.beacon().slot;
    let execution = store
        .finalized_header
        .execution()
        .expect("Execution payload doesn't exist.");

    let storage_slots = contract_storage
        .iter()
        .flat_map(|contract_storage| {
            verify_storage_slot_proofs(*execution.state_root(), contract_storage)
                .expect("Storage slot proofs failed to verify.")
        })
        .collect();

    let proof_outputs = ExecutionHeaderProofOutputs {
        executionStateRoot: *execution.state_root(),
        executionBlockNumber: U256::from(*execution.block_number()),
        executionBlockHash: *execution.block_hash(),
        executionReceiptsRoot: *execution.receipts_root(),
        newHeader: header,
        nextSyncCommitteeHash: next_sync_committee_hash,
        newHead: U256::from(head),
        prevHeader: prev_header,
        prevHead: U256::from(prev_head),
        syncCommitteeHash: sync_committee_hash,
        prevSyncCommitteeHash: prev_sync_committee_hash,
        storageSlots: storage_slots,
    };

    sp1_zkvm::io::commit_slice(&proof_outputs.abi_encode());
}
