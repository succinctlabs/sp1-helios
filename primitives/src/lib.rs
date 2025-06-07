use crate::types::{ContractStorage, StorageSlot};
use alloy_primitives::{keccak256, Bytes, B256};
use alloy_rlp::Encodable;
use alloy_trie::{proof, Nibbles};
use anyhow::Result;

pub mod types;

/// Verify the storage slot proofs for a given contract against the execution state root.
///
/// This function will:
/// - Verify the contracts [`alloy_trie::TrieAccount`] is correct and included in the execution state root.
/// - Verify each storage slot is correct and included in the execution state root.
pub fn verify_storage_slot_proofs(
    execution_state_root: B256,
    contract_storage: &ContractStorage,
) -> Result<Vec<StorageSlot>> {
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
        anyhow::bail!(
            "Could not verify the contract's `TrieAccount` in the global MPT for address {}: {}",
            hex::encode(contract_storage.address),
            e
        );
    }

    // 2) Now that we've verified the contract's `TrieAccount`, use it to verify each storage slot proof
    let mut verified_slots = Vec::with_capacity(contract_storage.storage_slots.len());
    for slot in &contract_storage.storage_slots {
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
            anyhow::bail!("Storage proof invalid for slot {}: {}", hex::encode(key), e);
        }

        verified_slots.push(StorageSlot {
            key,
            value: B256::from_slice(&value.to_be_bytes::<32>()),
            contractAddress: contract_storage.address,
        });
    }

    Ok(verified_slots)
}
