#![no_main]
sp1_zkvm::entrypoint!(main);

use alloy_primitives::{B256, U256};
use alloy_sol_types::SolValue;
use helios_consensus_core::{
    apply_finality_update, apply_update, verify_finality_update, verify_update,
};
use sp1_helios_primitives::types::{ProofInputs, ProofOutputs};
use tree_hash::TreeHash;

/// Program flow:
/// 1. Apply sync committee updates, if any
/// 2. Apply finality update
/// 3. Verify execution state root proof
/// 4. Asset all updates are valid
/// 5. Commit new state root, header, and sync committee for usage in the on-chain contract
///
/// This function is modeled off of the `sync` function in the `helios-ethereum` crate:
/// https://github.com/a16z/helios/blob/871c4d57fd6e2eb253581487c8a79bb3d486e0d1/ethereum/src/consensus.rs#L24
pub fn main() {
    let encoded_inputs = sp1_zkvm::io::read_vec();

    let ProofInputs {
        updates,
        finality_update,
        expected_current_slot,
        mut store,
        genesis_root,
        forks,
    } = serde_cbor::from_slice(&encoded_inputs).unwrap();

    // Get the initial sync committee hash. When verifying the proof, this is secured by the
    // `prevSyncCommitteeHash` field in the `ProofOutputs` struct.
    let prev_sync_committee_hash = store.current_sync_committee.tree_hash_root();

    let prev_header: B256 = store.finalized_header.beacon().tree_hash_root();
    let prev_head = store.finalized_header.beacon().slot;

    // 1. Verify and apply all generic updates
    for (index, update) in updates.iter().enumerate() {
        println!("Verifying update {} of {}.", index + 1, updates.len());
        verify_update(update, expected_current_slot, &store, genesis_root, &forks)
            .expect("Update is invalid!");
        apply_update(&mut store, update);
    }

    // 2. Verify and apply finality update
    verify_finality_update(
        &finality_update,
        expected_current_slot,
        &store,
        genesis_root,
        &forks,
    )
    .expect("Finality update failed to verify.");

    apply_finality_update(&mut store, &finality_update);

    // Ensure the new head is greater than the previous head. This guarantees that the finality
    // update was correctly applied.
    assert!(store.finalized_header.beacon().slot > prev_head);

    // 3. Commit new state root, header, and sync committee.
    let header: B256 = store.finalized_header.beacon().tree_hash_root();
    let sync_committee_hash: B256 = store.current_sync_committee.tree_hash_root();
    let next_sync_committee_hash: B256 = match &mut store.next_sync_committee {
        Some(next_sync_committee) => next_sync_committee.tree_hash_root(),
        None => B256::ZERO,
    };
    let head = store.finalized_header.beacon().slot;

    let proof_outputs = ProofOutputs {
        executionStateRoot: *store
            .finalized_header
            .execution()
            .expect("Execution payload doesn't exist.")
            .state_root(),
        newHeader: header,
        nextSyncCommitteeHash: next_sync_committee_hash,
        newHead: U256::from(head),
        prevHeader: prev_header,
        prevHead: U256::from(prev_head),
        syncCommitteeHash: sync_committee_hash,
        prevSyncCommitteeHash: prev_sync_committee_hash,
    };
    sp1_zkvm::io::commit_slice(&proof_outputs.abi_encode());
}
