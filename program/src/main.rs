#![no_main]
sp1_zkvm::entrypoint!(main);

use alloy_primitives::{B256, U256};
use alloy_sol_types::SolValue;
use consensus_core::{apply_finality_update, apply_update, verify_finality_update, verify_update};
use sp1_helios_primitives::types::{ProofInputs, ProofOutputs};
use ssz_rs::prelude::*;
use tree_hash::TreeHash;

/// The merkle branch index & depth for an Ethereum execution state root proof.
const MERKLE_BRANCH_INDEX: usize = 802;
const MERKLE_BRANCH_DEPTH: usize = 9;

/// Program flow:
/// 1. Apply sync committee updates, if any
/// 2. Apply finality update
/// 3. Verify execution state root proof
/// 4. Asset all updates are valid
/// 5. Commit new state root, header, and sync committee for usage in the on-chain contract
pub fn main() {
    let encoded_inputs = sp1_zkvm::io::read_vec();

    let ProofInputs {
        updates,
        finality_update,
        expected_current_slot,
        mut store,
        genesis_root,
        forks,
        execution_state_proof,
    } = serde_cbor::from_slice(&encoded_inputs).unwrap();

    let prev_header: B256 = store.finalized_header.beacon.tree_hash_root();
    let prev_head = store.finalized_header.beacon.slot;

    // 1. Apply sync committee updates, if any
    for (index, update) in updates.iter().enumerate() {
        println!("Processing update {} of {}.", index + 1, updates.len());
        let update_is_valid =
            verify_update(update, expected_current_slot, &store, genesis_root, &forks).is_ok();

        if !update_is_valid {
            panic!("Update {} is invalid!", index + 1);
        }
        println!("Update {} is valid.", index + 1);
        apply_update(&mut store, update);
    }

    // 2. Apply finality update
    let finality_update_is_valid = verify_finality_update(
        &finality_update,
        expected_current_slot,
        &store,
        genesis_root,
        &forks,
    )
    .is_ok();
    if !finality_update_is_valid {
        panic!("Finality update is invalid!");
    }
    println!("Finality update is valid.");

    apply_finality_update(&mut store, &finality_update);

    // 3. Verify execution state root proof
    let execution_state_branch_nodes: Vec<Node> = execution_state_proof
        .execution_state_branch
        .iter()
        .map(|b| Node::try_from(b.as_ref()).unwrap())
        .collect();

    let execution_state_proof_is_valid = is_valid_merkle_branch(
        &Node::try_from(execution_state_proof.execution_state_root.as_ref()).unwrap(),
        execution_state_branch_nodes.iter(),
        MERKLE_BRANCH_DEPTH,
        MERKLE_BRANCH_INDEX,
        &Node::try_from(store.finalized_header.beacon.body_root.as_ref()).unwrap(),
    );
    if !execution_state_proof_is_valid {
        panic!("Execution state root proof is invalid!");
    }
    println!("Execution state root proof is valid.");

    // 4. Commit new state root, header, and sync committee for usage in the on-chain contract
    let header: B256 = store.finalized_header.beacon.tree_hash_root();
    let sync_committee_hash: B256 = store.current_sync_committee.tree_hash_root();
    let next_sync_committee_hash: B256 = match &mut store.next_sync_committee {
        Some(next_sync_committee) => next_sync_committee.tree_hash_root(),
        None => B256::ZERO,
    };
    let head = store.finalized_header.beacon.slot;

    let proof_outputs = ProofOutputs {
        executionStateRoot: execution_state_proof.execution_state_root,
        newHeader: header,
        nextSyncCommitteeHash: next_sync_committee_hash,
        newHead: U256::from(head),
        prevHeader: prev_header,
        prevHead: U256::from(prev_head),
        syncCommitteeHash: sync_committee_hash,
    };
    sp1_zkvm::io::commit_slice(&proof_outputs.abi_encode());
}
