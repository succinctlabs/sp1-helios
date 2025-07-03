use alloy::primitives::{Address, B256};
use anyhow::Result;
use sp1_sdk::SP1ProofWithPublicValues;
use std::collections::{HashMap, HashSet};
use std::{future::Future, sync::Arc};
use tokio::sync::{mpsc, oneshot, Mutex};

pub struct OperatorHandle {
    storage_slot_config: Arc<Mutex<HashMap<Address, HashSet<B256>>>>,
    shutdown: oneshot::Sender<()>,
    storage_proof_tx: mpsc::UnboundedSender<StorageProofRequest>,
}

pub(crate) struct StorageProofRequest {
    pub(crate) block_number: u64,
    pub(crate) contract_keys: Vec<ContractKeys>,
    pub(crate) tx: oneshot::Sender<Result<SP1ProofWithPublicValues>>,
}

/// A list of contract and storage slots.
pub struct ContractKeys {
    pub address: Address,
    pub storage_slots: Vec<B256>,
}

impl OperatorHandle {
    pub(crate) fn new(
        storage_slot_config: Arc<Mutex<HashMap<Address, HashSet<B256>>>>,
        shutdown: oneshot::Sender<()>,
        storage_proof_tx: mpsc::UnboundedSender<StorageProofRequest>,
    ) -> Self {
        Self {
            storage_slot_config,
            shutdown,
            storage_proof_tx,
        }
    }

    /// Add a storage slot to the operator.
    pub async fn add_storage_slot(&self, address: Address, storage_slot: B256) {
        let mut storage_slot_config = self.storage_slot_config.lock().await;
        storage_slot_config
            .entry(address)
            .or_insert_with(HashSet::new)
            .insert(storage_slot);
    }

    /// Remove a storage slot from the operator.
    pub async fn remove_storage_slot(&self, address: Address, storage_slot: B256) {
        let mut storage_slot_config = self.storage_slot_config.lock().await;
        storage_slot_config
            .entry(address)
            .or_insert_with(HashSet::new)
            .remove(&storage_slot);
    }

    /// Remove an address from the operator.
    pub async fn remove_address(&self, address: Address) {
        let mut storage_slot_config = self.storage_slot_config.lock().await;
        storage_slot_config.remove(&address);
    }

    /// Modify the storage slot config in place.
    pub async fn modify_storage_slots_in_place<F, Fut, Res>(&self, func: F) -> Res
    where
        F: Fn(&mut HashMap<Address, HashSet<B256>>) -> Fut,
        Fut: Future<Output = Res>,
    {
        let mut storage_slot_config = self.storage_slot_config.lock().await;
        func(&mut storage_slot_config).await
    }

    /// Get a proof for a given address and storage slots.
    pub async fn get_proof_for(
        &self,
        block_number: u64,
        address: Address,
        storage_slot: &[B256],
    ) -> Result<SP1ProofWithPublicValues> {
        let (tx, rx) = oneshot::channel();
        if let Err(e) = self.storage_proof_tx.send(StorageProofRequest {
            block_number,
            contract_keys: vec![ContractKeys {
                address,
                storage_slots: storage_slot.to_vec(),
            }],
            tx,
        }) {
            tracing::error!("Failed to send storage proof request: {:?}", e);
        }

        rx.await?
    }

    /// Get proofs for a given block number and a list of contract keys.
    pub async fn get_proofs_for(
        &self,
        block_number: u64,
        contract_keys: Vec<ContractKeys>,
    ) -> Result<SP1ProofWithPublicValues> {
        let (tx, rx) = oneshot::channel();
        if let Err(e) = self.storage_proof_tx.send(StorageProofRequest {
            block_number,
            contract_keys,
            tx,
        }) {
            tracing::error!("Failed to send storage proof request: {:?}", e);
        }

        rx.await?
    }

    /// Shutdown the operator.
    pub async fn shutdown(self) {
        if self.shutdown.send(()).is_err() {
            tracing::error!("Failed to send shutdown signal");
        }
    }
}
