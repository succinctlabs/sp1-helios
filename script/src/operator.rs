use crate::handle::ContractKeys;
use crate::handle::{OperatorHandle, StorageProofRequest};
use crate::{get_client, get_updates};
use alloy::primitives::{Address, B256};
use alloy::providers::{Provider, WalletProvider};
use alloy::sol_types::SolType;
use anyhow::{Context, Result};
use helios_consensus_core::consensus_spec::MainnetConsensusSpec;
use helios_ethereum::consensus::Inner;
use helios_ethereum::rpc::http_rpc::HttpRpc;
use helios_ethereum::rpc::ConsensusRpc;
use sp1_helios_primitives::types::{
    ContractStorage, ProofInputs, ProofOutputs, SP1Helios, StorageSlotWithProof,
};
use sp1_helios_primitives::verify_storage_slot_proofs;
use sp1_sdk::{
    EnvProver, HashableKey, ProverClient, SP1ProofWithPublicValues, SP1ProvingKey, SP1Stdin,
};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};

use std::collections::{HashMap, HashSet};
use tokio::sync::{mpsc, oneshot, Mutex};

const LIGHTCLIENT_ELF: &[u8] = include_bytes!("../../elf/light_client");
const STORAGE_ELF: &[u8] = include_bytes!("../../elf/storage");

pub struct SP1HeliosOperator<P> {
    client: Arc<EnvProver>,
    provider: P,
    lightclient_pk: Arc<SP1ProvingKey>,
    storage_slots_pk: Arc<SP1ProvingKey>,
    contract_address: Address,
    storage_slots_to_fetch: Arc<Mutex<HashMap<Address, HashSet<B256>>>>,
    source_chain_id: u64,
    source_consensus_rpc: String,
}

impl<P> SP1HeliosOperator<P>
where
    P: Provider + WalletProvider,
{
    /// Fetch values and generate an 'update' proof for the SP1 Helios contract.
    async fn request_update(
        &self,
        client: Inner<MainnetConsensusSpec, HttpRpc>,
    ) -> Result<Option<SP1ProofWithPublicValues>> {
        let contract = SP1Helios::new(self.contract_address, &self.provider);
        let head: u64 = contract
            .head()
            .call()
            .await
            .context("Failed to get head from contract")?
            .try_into()
            .expect("Failed to convert head to u64, this is a bug.");

        let mut stdin = SP1Stdin::new();

        // Setup client.
        let updates = get_updates(&client).await;
        let finality_update = client.rpc.get_finality_update().await.unwrap();

        // Check if contract is up to date
        let latest_block = finality_update.finalized_header().beacon().slot;
        if latest_block <= head {
            info!("Contract is up to date. Nothing to update.");
            return Ok(None);
        } else if !latest_block.is_multiple_of(32) {
            info!("Attempted to commit to a non-checkpoint slot: {latest_block}. Skipping update.");
            return Ok(None);
        }

        info!(
            "Updating to new head block: {:?} from {:?}",
            latest_block, head
        );

        let latest_execution_block_number = client
            .store
            .finalized_header
            .execution()
            .expect("Failed to get (finalized) execution header from store")
            .block_number();

        // Fetch the contract storage, if any.
        let contract_storage = self
            .get_storage_slots(*latest_execution_block_number)
            .await?;

        // Create program inputs
        let expected_current_slot = client.expected_current_slot();
        let inputs = ProofInputs {
            updates,
            finality_update,
            expected_current_slot,
            store: client.store.clone(),
            genesis_root: client.config.chain.genesis_root,
            forks: client.config.forks.clone(),
            contract_storage,
        };
        let encoded_proof_inputs = serde_cbor::to_vec(&inputs)?;
        stdin.write_slice(&encoded_proof_inputs);

        // Generate proof.
        let proof = tokio::task::spawn_blocking({
            let client = self.client.clone();
            let pk = self.lightclient_pk.clone();

            move || client.prove(&pk, &stdin).plonk().run()
        })
        .await??;

        info!("Attempting to update to new head block: {:?}", latest_block);
        Ok(Some(proof))
    }

    /// Relay an update proof to the SP1 Helios contract.
    async fn relay_update(&self, proof: SP1ProofWithPublicValues) -> Result<()> {
        let contract = SP1Helios::new(self.contract_address, &self.provider);

        let nonce = self
            .provider
            .get_transaction_count(self.provider.default_signer_address())
            .await?;

        // Wait for 3 required confirmations with a timeout of 60 seconds.
        const NUM_CONFIRMATIONS: u64 = 3;
        const TIMEOUT_SECONDS: u64 = 60;

        let po = ProofOutputs::abi_decode(proof.public_values.as_slice())?;

        let tx = contract.update(
            proof.bytes().into(),
            po.newHead,
            po.newHeader,
            po.executionStateRoot,
            po.executionBlockNumber,
            po.syncCommitteeHash,
            po.nextSyncCommitteeHash,
            po.storageSlots,
        );

        let receipt = tx
            .nonce(nonce)
            .send()
            .await?
            .with_required_confirmations(NUM_CONFIRMATIONS)
            .with_timeout(Some(Duration::from_secs(TIMEOUT_SECONDS)))
            .get_receipt()
            .await?;

        // If status is false, it reverted.
        if !receipt.status() {
            error!("Transaction reverted!");
            return Err(anyhow::anyhow!("Transaction reverted!"));
        }

        info!(
            "Successfully updated to new head block! Tx hash: {:?}",
            receipt.transaction_hash
        );

        Ok(())
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

    /// Check if the vkeys of the light client and storage slot programs are correct and match the ones in the contract.
    async fn check_vkeys(&self) -> Result<()> {
        let contract = SP1Helios::new(self.contract_address, &self.provider);
        let contract_lightclient_vkey = contract.lightClientVkey().call().await?;
        let contract_storage_slot_vkey = contract.storageSlotVkey().call().await?;

        if self.lightclient_pk.vk.bytes32_raw() != contract_lightclient_vkey {
            return Err(anyhow::anyhow!("Light client vkey mismatch"));
        }

        if self.storage_slots_pk.vk.bytes32_raw() != contract_storage_slot_vkey {
            return Err(anyhow::anyhow!("Storage slot vkey mismatch"));
        }

        Ok(())
    }
}

impl<P> SP1HeliosOperator<P>
where
    P: Provider + WalletProvider,
{
    /// Create a new SP1 Helios operator.
    pub async fn new(
        provider: P,
        contract_address: Address,
        consensus_rpc: String,
        chain_id: u64,
    ) -> Self {
        let client = ProverClient::from_env();

        tracing::info!("Setting up light client program...");
        let (lightclient_pk, _) = client.setup(LIGHTCLIENT_ELF);
        tracing::info!("Setting up storage slots program...");
        let (storage_slots_pk, _) = client.setup(STORAGE_ELF);

        let this = Self {
            client: Arc::new(client),
            provider,
            lightclient_pk: Arc::new(lightclient_pk),
            storage_slots_pk: Arc::new(storage_slots_pk),
            contract_address,
            storage_slots_to_fetch: Arc::new(Mutex::new(HashMap::new())),
            source_chain_id: chain_id,
            source_consensus_rpc: consensus_rpc,
        };

        this.check_vkeys()
            .await
            .expect("Failed to create operator: vkeys mismatch");

        this
    }

    /// Run a single iteration of the operator, possibly posting a new update on chain.
    pub async fn run_once(&self) -> Result<()> {
        let contract = SP1Helios::new(self.contract_address, &self.provider);

        // Get the current slot from the contract
        let slot = contract
            .head()
            .call()
            .await
            .context("Failed to get head from contract")?
            .try_into()
            .expect("Failed to convert head to u64, this is a bug.");

        // Fetch the checkpoint at that slot
        let client =
            get_client(Some(slot), &self.source_consensus_rpc, self.source_chain_id).await?;

        assert_eq!(
            client.store.finalized_header.beacon().slot,
            slot,
            "Bootstrapped client has mismatched finalized slot, this is a bug!"
        );

        // Request an update
        match self.request_update(client).await {
            Ok(Some(proof)) => {
                self.relay_update(proof).await?;
            }
            Ok(None) => {
                // Contract is up to date. Nothing to update.
            }
            Err(e) => {
                error!("Header range request failed: {}", e);
            }
        }

        Ok(())
    }

    pub async fn prove_storage_slots(
        &self,
        block_number: u64,
        contract_keys: Vec<ContractKeys>,
    ) -> Result<SP1ProofWithPublicValues> {
        let Some(block) = self.provider.get_block(block_number.into()).await? else {
            anyhow::bail!("Failed to get block {block_number} from provider, this was expected to valid since the store claimed to have this block finalized.");
        };

        let proofs = contract_keys.into_iter().map(|keys| {
            self.get_storage_slot_proof_for_contract(
                block.header.state_root,
                block_number,
                keys.address,
                keys.storage_slots,
            )
        });

        let proofs = futures::future::try_join_all(proofs).await?;

        let mut stdin = SP1Stdin::new();
        stdin.write(&proofs);
        stdin.write(&block.header.state_root);

        let proof = tokio::task::spawn_blocking({
            let client = self.client.clone();
            let pk = self.storage_slots_pk.clone();

            move || client.prove(&pk, &stdin).plonk().run()
        })
        .await??;

        Ok(proof)
    }
}

impl<P> SP1HeliosOperator<P>
where
    P: Provider + WalletProvider + 'static,
{
    /// Start the operator in [tokio] task, running indefinitely and retrying on failure.
    pub fn run(self, loop_delay: Duration) -> OperatorHandle {
        info!("Starting SP1 Helios operator");

        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        let (storage_proof_tx, mut storage_proof_rx) = mpsc::unbounded_channel();
        let mut tick = tokio::time::interval(loop_delay);

        let operator_handle = OperatorHandle::new(
            self.storage_slots_to_fetch.clone(),
            shutdown_tx,
            storage_proof_tx,
        );

        tokio::spawn(async move {
            // Do the first iteration right away.
            if let Err(e) = self.run_once().await {
                error!("Error running operator: {}", e);
            }

            let this = Arc::new(self);
            loop {
                let clone = this.clone();

                tokio::select! {
                    _ = tick.tick() => {
                        tokio::spawn(async move {
                            if let Err(e) = clone.run_once().await {
                                error!("Error running operator: {:?}", e);
                            }
                        });
                    }
                    req = storage_proof_rx.recv() => {
                        tokio::spawn(async move {
                            match req {
                                Some(StorageProofRequest { block_number, contract_keys, tx }) => {
                                    let proof_result = clone.prove_storage_slots(block_number, contract_keys).await.inspect_err(|e| {
                                        tracing::error!("Error proving storage slot: {:?}", e);
                                    });

                                    if let Err(e) = tx.send(proof_result) {
                                        tracing::error!("Failed to send storage proof: {:?}", e);
                                    }
                                }
                                None => {
                                    tracing::error!("State proof channel closed");
                                }
                            }
                        });

                    }
                    _ = &mut shutdown_rx => {
                        info!("Received shutdown signal, shutting down");
                        break;
                    }
                }
            }
        });

        operator_handle
    }
}
