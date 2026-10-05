//! Preserve the pre-Gloas collector for before/after regression tests.
//!
//! The two private collection method bodies are copied unchanged from
//! script/src/operator.rs at cb807dbc1eb12889ec6ddd77c3258433a8369054.
//! Only the surrounding type is reduced to the fields those methods access.

use std::collections::{HashMap, HashSet};

use alloy::primitives::{Address, B256};
use alloy::providers::Provider;
use anyhow::{Context, Result};
use sp1_helios_primitives::{
    types::{ContractStorage, StorageSlotWithProof},
    verify_storage_slot_proofs,
};
use tokio::sync::Mutex;

pub struct LegacyCollector<P> {
    provider: P,
    storage_slots_to_fetch: Mutex<HashMap<Address, HashSet<B256>>>,
}

impl<P: Provider> LegacyCollector<P> {
    pub fn new(provider: P, address: Address, key: B256) -> Self {
        Self {
            provider,
            storage_slots_to_fetch: Mutex::new(HashMap::from([(address, HashSet::from([key]))])),
        }
    }

    pub async fn collect(&self, starting_block: u64) -> Result<Vec<ContractStorage>> {
        self.get_storage_slots(starting_block).await
    }

    async fn get_storage_slots(&self, block_number: u64) -> Result<Vec<ContractStorage>> {
        let storage_slots_to_fetch = self.storage_slots_to_fetch.lock().await;
        if storage_slots_to_fetch.is_empty() {
            return Ok(vec![]);
        }

        let Some(block) = self.provider.get_block(block_number.into()).await? else {
            anyhow::bail!("Failed to get block {block_number} from provider, this was expected to valid since the store claimed to have this block finalized.");
        };

        let futs = storage_slots_to_fetch.iter().map(|(contract, keys)| {
            self.get_storage_slot_proof_for_contract(
                block.header.state_root,
                block_number,
                *contract,
                keys.iter().copied().collect(),
            )
        });

        futures::future::try_join_all(futs).await
    }

    async fn get_storage_slot_proof_for_contract(
        &self,
        state_root: B256,
        block_number: u64,
        contract_address: Address,
        keys: Vec<B256>,
    ) -> Result<ContractStorage> {
        let proof = self
            .provider
            .get_proof(contract_address, keys)
            .number(block_number)
            .await?;

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

        verify_storage_slot_proofs(state_root, &contract_storage).context(format!(
            "Preflight storage slot proofs failed to verify for contract {contract_address:?}"
        ))?;

        Ok(contract_storage)
    }
}
