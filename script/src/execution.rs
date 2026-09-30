//! Fetch source execution data for the exact header finalized by the SP1 input sequence.

use std::time::Duration;

use alloy::rpc::types::EIP1186AccountProofResponse;
use alloy_consensus::Header;
use alloy_primitives::{Address, B256};
use anyhow::{ensure, Context, Result};
use helios_consensus_core::{
    apply_finality_update, apply_update, types::LightClientHeader, verify_finality_update,
    verify_update,
};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use sp1_helios_primitives::{
    execution::{verified_execution_fields, ExecutionFields},
    types::{ContractStorage, ProofInputs, StorageSlotWithProof},
    verify_storage_slot_proofs,
};

/// Read-only execution RPC for the source chain.
pub struct ExecutionRpc {
    url: reqwest::Url,
    client: reqwest::Client,
}

impl ExecutionRpc {
    /// Construct a source RPC client with bounded requests.
    pub fn new(url: &str) -> Result<Self> {
        Ok(Self {
            url: url.parse().context("Invalid source execution RPC URL")?,
            client: reqwest::Client::builder()
                .user_agent("sp1-helios")
                .timeout(Duration::from_secs(30))
                .build()?,
        })
    }

    async fn request<T: DeserializeOwned>(&self, method: &str, params: Value) -> Result<T> {
        let response: Value = self
            .client
            .post(self.url.clone())
            .json(&json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        if let Some(error) = response.get("error") {
            anyhow::bail!("Source execution RPC {method} failed: {error}");
        }
        let result = response
            .get("result")
            .context("RPC response has no result")?;
        ensure!(
            !result.is_null(),
            "Source execution RPC {method} returned no data"
        );
        serde_json::from_value(result.clone()).with_context(|| format!("Invalid {method} response"))
    }

    /// Reject a source endpoint configured for another chain.
    pub async fn check_chain_id(&self, expected: u64) -> Result<()> {
        let value: String = self.request("eth_chainId", json!([])).await?;
        let actual = u64::from_str_radix(value.trim_start_matches("0x"), 16)?;
        ensure!(
            actual == expected,
            "Source chain ID mismatch: expected {expected}, got {actual}"
        );
        Ok(())
    }

    /// Fetch a full header and authenticate its canonical RLP hash.
    pub async fn header_by_hash(&self, hash: B256) -> Result<Header> {
        let header: Header = self
            .request("eth_getBlockByHash", json!([hash, false]))
            .await?;
        ensure!(
            header.hash_slow() == hash,
            "Source execution header does not match requested hash {hash}"
        );
        Ok(header)
    }

    /// Fetch a header for a standalone storage-proof request.
    pub async fn header_by_number(&self, number: u64) -> Result<Header> {
        let header: Header = self
            .request(
                "eth_getBlockByNumber",
                json!([format!("0x{number:x}"), false]),
            )
            .await?;
        ensure!(
            header.number == number,
            "Execution block number mismatch: expected {number}, got {}",
            header.number
        );
        Ok(header)
    }

    /// Fetch account/storage proofs and verify them against the selected source state root.
    pub async fn storage_proof(
        &self,
        state_root: B256,
        block_number: u64,
        address: Address,
        keys: Vec<B256>,
    ) -> Result<ContractStorage> {
        let proof: EIP1186AccountProofResponse = self
            .request(
                "eth_getProof",
                json!([address, keys, format!("0x{block_number:x}")]),
            )
            .await?;
        ensure!(proof.address == address, "Storage proof address mismatch");
        ensure!(
            proof.storage_proof.len() == keys.len(),
            "Storage proof omitted requested slots"
        );
        for key in &keys {
            ensure!(
                proof.storage_proof.iter().any(|p| p.key.as_b256() == *key),
                "Storage proof omitted requested slot {key}"
            );
        }
        let contract_storage = ContractStorage {
            address: proof.address,
            value: alloy_trie::TrieAccount {
                nonce: proof.nonce,
                balance: proof.balance,
                storage_root: proof.storage_hash,
                code_hash: proof.code_hash,
            },
            mpt_proof: proof.account_proof,
            storage_slots: proof
                .storage_proof
                .into_iter()
                .map(|p| StorageSlotWithProof {
                    key: p.key.as_b256(),
                    value: p.value,
                    mpt_proof: p.proof,
                })
                .collect(),
        };
        verify_storage_slot_proofs(state_root, &contract_storage)
            .with_context(|| format!("Invalid source storage proof for {address}"))?;
        Ok(contract_storage)
    }
}

/// Preview the guest's update sequence without modifying its anchored starting store.
pub fn preview_finalized_header(inputs: &ProofInputs) -> Result<LightClientHeader> {
    let mut store = inputs.store.clone();
    store.next_sync_committee = None;
    for update in &inputs.updates {
        verify_update(
            update,
            inputs.expected_current_slot,
            &store,
            inputs.genesis_root,
            &inputs.forks,
        )
        .map_err(|e| anyhow::anyhow!("Invalid update: {e}"))?;
        apply_update(&mut store, update);
    }
    verify_finality_update(
        &inputs.finality_update,
        inputs.expected_current_slot,
        &store,
        inputs.genesis_root,
        &inputs.forks,
    )
    .map_err(|e| anyhow::anyhow!("Invalid finality update: {e}"))?;
    apply_finality_update(&mut store, &inputs.finality_update);
    let slot = store.finalized_header.beacon().slot;
    ensure!(
        slot > inputs.store.finalized_header.beacon().slot,
        "Updates do not advance finalized head"
    );
    ensure!(
        slot.is_multiple_of(32),
        "Finalized head is not a checkpoint slot"
    );
    Ok(store.finalized_header)
}

/// Attach a Gloas witness for the actual finalized store, then return authenticated fields.
pub async fn fill_execution_header(
    inputs: &mut ProofInputs,
    rpc: &ExecutionRpc,
) -> Result<(u64, ExecutionFields)> {
    let finalized = preview_finalized_header(inputs)?;
    inputs.execution_header = match &finalized {
        LightClientHeader::Gloas(header) => {
            Some(rpc.header_by_hash(header.execution_block_hash).await?)
        }
        _ => None,
    };
    Ok((
        finalized.beacon().slot,
        verified_execution_fields(&finalized, inputs.execution_header.as_ref())?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_hash::TreeHash;

    #[test]
    fn preview_preserves_the_anchored_store_and_drops_untrusted_committee() {
        let mut inputs: ProofInputs =
            serde_cbor::from_slice(include_bytes!("../tests/fixtures/proof_inputs.cbor")).unwrap();
        let original_slot = inputs.store.finalized_header.beacon().slot;
        let original_root = inputs.store.finalized_header.beacon().tree_hash_root();
        let finalized = preview_finalized_header(&inputs).unwrap();
        assert!(finalized.beacon().slot > original_slot);
        assert_eq!(
            inputs.store.finalized_header.beacon().tree_hash_root(),
            original_root
        );
        assert_eq!(inputs.store.finalized_header.beacon().slot, original_slot);

        inputs.store.next_sync_committee = Some(inputs.store.current_sync_committee.clone());
        let poisoned_preview = preview_finalized_header(&inputs).unwrap();
        assert_eq!(poisoned_preview, finalized);
        assert!(inputs.store.next_sync_committee.is_some());
        verified_execution_fields(&finalized, None).unwrap();
    }
}
