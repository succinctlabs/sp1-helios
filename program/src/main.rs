#![no_main]
sp1_zkvm::entrypoint!(main);

use alloy_primitives::{keccak256, Bytes, B256, U256};
use alloy_rlp::Encodable;
use alloy_sol_types::SolValue;
use alloy_trie::{proof, Nibbles};
use helios_consensus_core::{
    apply_finality_update, apply_update, verify_finality_update, verify_update,
};
use sp1_helios_primitives::types::{ContractStorage, ProofInputs, ProofOutputs, StorageSlot};
use tree_hash::TreeHash;

/// Program flow:
/// 1n. Apply sync committee updates, if ay
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
        contract_storage,
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
    assert!(
        store.finalized_header.beacon().slot > prev_head,
        "New head is not greater than previous head."
    );
    assert!(
        store.finalized_header.beacon().slot % 32 == 0,
        "New head is not a checkpoint slot."
    );

    // 3. Commit new state root, header, and sync committee.
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
        .into_iter()
        .flat_map(|contract_storage| {
            verify_storage_slot_proofs(*execution.state_root(), contract_storage)
        })
        .collect();

    let proof_outputs = ProofOutputs {
        executionStateRoot: *execution.state_root(),
        newHeader: header,
        executionBlockNumber: U256::from(*execution.block_number()),
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

fn verify_storage_slot_proofs(
    execution_state_root: B256,
    contract_storage: ContractStorage,
) -> Vec<StorageSlot> {
    // Convert the contract address into nibbles for the global MPT proof
    // We need to keccak256 the address before converting to nibbles for the MPT proof
    let address_hash = keccak256(contract_storage.address.as_slice());
    let address_nibbles = Nibbles::unpack(Bytes::copy_from_slice(address_hash.as_ref()));
    // RLP-encode the `TrieAccount`. This is what's actually stored in the global MPT
    let mut rlp_encoded_trie_account = Vec::new();
    contract_storage.value.encode(&mut rlp_encoded_trie_account);

    // 1) Verify the contract's account node in the global MPT:
    //    We expect to find `rlp_encoded_trie_account` as the trie value for this address.
    if let Err(e) = proof::verify_proof(
        execution_state_root,
        address_nibbles,
        Some(rlp_encoded_trie_account),
        &contract_storage.mpt_proof,
    ) {
        panic!(
            "Could not verify the contract's `TrieAccount` in the global MPT for address {}: {}",
            hex::encode(contract_storage.address),
            e
        );
    }

    // 2) Now that we've verified the contract's `TrieAccount`, use it to verify each storage slot proof
    let mut verified_slots = Vec::with_capacity(contract_storage.storage_slots.len());
    for slot in contract_storage.storage_slots {
        let key = slot.key;
        let value = slot.value;
        // We need to keccak256 the slot key before converting to nibbles for the MPT proof
        let key_hash = keccak256(key.as_slice());
        let key_nibbles = Nibbles::unpack(Bytes::copy_from_slice(key_hash.as_ref()));
        // RLP-encode expected value. This is what's actually stored in the contract MPT
        let mut rlp_encoded_value = Vec::new();
        value.encode(&mut rlp_encoded_value);

        // Verify the storage proof under the *contract's* storage root
        if let Err(e) = proof::verify_proof(
            contract_storage.value.storage_root,
            key_nibbles,
            Some(rlp_encoded_value),
            &slot.mpt_proof,
        ) {
            panic!("Storage proof invalid for slot {}: {}", hex::encode(key), e);
        }

        verified_slots.push(StorageSlot {
            key,
            value: B256::from_slice(&value.to_be_bytes::<32>()),
            contractAddress: contract_storage.address,
        });
    }

    verified_slots
}
