use crate::execution::{fill_execution_header, ExecutionRpc};
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
    ContractStorage, ExecutionHeaderProofOutputs, ProofInputs, ProofOutputs, SP1Helios,
};
use sp1_sdk::{
    network::{signer::NetworkSigner, FulfillmentStrategy, NetworkMode},
    HashableKey, NetworkProver, ProveRequest, Prover, ProverClient, ProvingKey, SP1ProofMode,
    SP1ProofWithPublicValues, SP1ProvingKey, SP1Stdin,
};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};

use std::collections::{HashMap, HashSet};
use tokio::sync::{mpsc, oneshot, Mutex};

const LIGHT_CLIENT_ELF: &[u8] = include_bytes!("../../elf/light_client");
const EXECUTION_HEADER_ELF: &[u8] = include_bytes!("../../elf/execution_header");
const STORAGE_ELF: &[u8] = include_bytes!("../../elf/storage");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionCommitment {
    StateRootOnly,
    HeaderWithReceipts,
}

impl ExecutionCommitment {
    fn elf(self) -> &'static [u8] {
        match self {
            Self::StateRootOnly => LIGHT_CLIENT_ELF,
            Self::HeaderWithReceipts => EXECUTION_HEADER_ELF,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::StateRootOnly => "state root only",
            Self::HeaderWithReceipts => "header with receipts",
        }
    }
}

pub struct SP1HeliosOperator<P> {
    client: Arc<NetworkProver>,
    provider: P,
    update_pk: Arc<SP1ProvingKey>,
    storage_slots_pk: Arc<SP1ProvingKey>,
    contract_address: Address,
    storage_slots_to_fetch: Arc<Mutex<HashMap<Address, HashSet<B256>>>>,
    source_chain_id: u64,
    source_consensus_rpc: String,
    source_execution_rpc: ExecutionRpc,
    execution_commitment: ExecutionCommitment,
    fulfillment_strategy: FulfillmentStrategy,
    proof_mode: SP1ProofMode,
}

pub struct ProverSettings {
    pub network_signer: NetworkSigner,
    pub fulfillment_strategy: FulfillmentStrategy,
    pub proof_mode: SP1ProofMode,
}

pub fn parse_fulfillment_strategy(value: &str) -> Result<FulfillmentStrategy> {
    match value.to_ascii_lowercase().as_str() {
        "auction" => Ok(FulfillmentStrategy::Auction),
        "hosted" => Ok(FulfillmentStrategy::Hosted),
        "reserved" => Ok(FulfillmentStrategy::Reserved),
        _ => anyhow::bail!(
            "invalid SP1_HELIOS_FULFILLMENT_STRATEGY '{value}'; expected auction, hosted, or reserved"
        ),
    }
}

pub fn network_mode_for(strategy: FulfillmentStrategy) -> NetworkMode {
    match strategy {
        FulfillmentStrategy::Auction => NetworkMode::Mainnet,
        FulfillmentStrategy::Hosted | FulfillmentStrategy::Reserved => NetworkMode::Reserved,
        FulfillmentStrategy::UnspecifiedFulfillmentStrategy => {
            unreachable!("parse_fulfillment_strategy rejects unspecified")
        }
    }
}

pub fn parse_proof_mode(value: &str) -> Result<SP1ProofMode> {
    match value.to_ascii_lowercase().as_str() {
        "plonk" => Ok(SP1ProofMode::Plonk),
        "groth16" => Ok(SP1ProofMode::Groth16),
        _ => anyhow::bail!("invalid SP1_HELIOS_PROOF_MODE '{value}'; expected plonk or groth16"),
    }
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
        let updates = get_updates(&client).await?;
        let finality_update = client
            .rpc
            .get_finality_update()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to fetch finality update: {e}"))?;

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

        // Create program inputs
        let expected_current_slot = client.expected_current_slot();
        let mut inputs = ProofInputs {
            updates,
            finality_update,
            expected_current_slot,
            store: client.store.clone(),
            genesis_root: client.config.chain.genesis_root,
            forks: client.config.forks.clone(),
            contract_storage: vec![],
            execution_header: None,
        };
        let (latest_block, execution) =
            fill_execution_header(&mut inputs, &self.source_execution_rpc).await?;
        inputs.contract_storage = self
            .get_storage_slots(execution.state_root, execution.block_number)
            .await?;
        let encoded_proof_inputs = serde_cbor::to_vec(&inputs)?;
        stdin.write_slice(&encoded_proof_inputs);

        // Generate proof.
        let proof = self
            .client
            .prove(&self.update_pk, stdin)
            .mode(self.proof_mode)
            .strategy(self.fulfillment_strategy)
            .await?;

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

        let receipt = match self.execution_commitment {
            ExecutionCommitment::StateRootOnly => {
                let po = ProofOutputs::abi_decode(proof.public_values.as_slice())?;
                contract
                    .update(
                        proof.bytes().into(),
                        po.newHead,
                        po.newHeader,
                        po.executionStateRoot,
                        po.executionBlockNumber,
                        po.syncCommitteeHash,
                        po.nextSyncCommitteeHash,
                        po.storageSlots,
                    )
                    .nonce(nonce)
                    .send()
                    .await?
                    .with_required_confirmations(NUM_CONFIRMATIONS)
                    .with_timeout(Some(Duration::from_secs(TIMEOUT_SECONDS)))
                    .get_receipt()
                    .await?
            }
            ExecutionCommitment::HeaderWithReceipts => {
                let po = ExecutionHeaderProofOutputs::abi_decode(proof.public_values.as_slice())?;
                contract
                    .updateExecutionHeader(proof.bytes().into(), po)
                    .nonce(nonce)
                    .send()
                    .await?
                    .with_required_confirmations(NUM_CONFIRMATIONS)
                    .with_timeout(Some(Duration::from_secs(TIMEOUT_SECONDS)))
                    .get_receipt()
                    .await?
            }
        };

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

    async fn get_storage_slots(
        &self,
        state_root: B256,
        block_number: u64,
    ) -> Result<Vec<ContractStorage>> {
        let storage_slots_to_fetch = self.storage_slots_to_fetch.lock().await;
        if storage_slots_to_fetch.is_empty() {
            return Ok(vec![]);
        }

        let futs = storage_slots_to_fetch.iter().map(|(contract, keys)| {
            self.source_execution_rpc.storage_proof(
                state_root,
                block_number,
                *contract,
                keys.iter().copied().collect(),
            )
        });

        futures::future::try_join_all(futs).await
    }

    /// Check if the vkeys of the light client and storage slot programs are correct and match the ones in the contract.
    async fn check_vkeys(&self) -> Result<()> {
        let contract = SP1Helios::new(self.contract_address, &self.provider);
        let contract_update_vkey = match self.execution_commitment {
            ExecutionCommitment::StateRootOnly => contract.lightClientVkey().call().await?,
            ExecutionCommitment::HeaderWithReceipts => {
                contract.executionHeaderVkey().call().await?
            }
        };
        let contract_storage_slot_vkey = contract.storageSlotVkey().call().await?;

        if self.update_pk.verifying_key().bytes32_raw() != contract_update_vkey {
            return Err(anyhow::anyhow!(
                "{} vkey mismatch",
                self.execution_commitment.label()
            ));
        }

        if self.storage_slots_pk.verifying_key().bytes32_raw() != contract_storage_slot_vkey {
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
        execution_rpc: String,
        chain_id: u64,
        execution_commitment: ExecutionCommitment,
        prover_settings: ProverSettings,
    ) -> Result<Self> {
        let source_execution_rpc = ExecutionRpc::new(&execution_rpc)?;
        source_execution_rpc
            .check_chain_id(chain_id)
            .await
            .context("Failed to validate source execution chain ID")?;
        let client = ProverClient::builder()
            .network_for(network_mode_for(prover_settings.fulfillment_strategy))
            .signer(prover_settings.network_signer)
            .build()
            .await;

        tracing::info!("Setting up {} program...", execution_commitment.label());
        let update_pk = client
            .setup(execution_commitment.elf().into())
            .await
            .context("Failed to setup update program")?;
        tracing::info!("Setting up storage slots program...");
        let storage_slots_pk = client
            .setup(STORAGE_ELF.into())
            .await
            .context("Failed to setup storage slots program")?;

        let this = Self {
            client: Arc::new(client),
            provider,
            update_pk: Arc::new(update_pk),
            storage_slots_pk: Arc::new(storage_slots_pk),
            contract_address,
            storage_slots_to_fetch: Arc::new(Mutex::new(HashMap::new())),
            source_chain_id: chain_id,
            source_consensus_rpc: consensus_rpc,
            source_execution_rpc,
            execution_commitment,
            fulfillment_strategy: prover_settings.fulfillment_strategy,
            proof_mode: prover_settings.proof_mode,
        };

        this.check_vkeys()
            .await
            .context("Failed to create operator: vkeys mismatch")?;

        Ok(this)
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
        let header = self
            .source_execution_rpc
            .header_by_number(block_number)
            .await?;

        let proofs = contract_keys.into_iter().map(|keys| {
            self.source_execution_rpc.storage_proof(
                header.state_root,
                block_number,
                keys.address,
                keys.storage_slots,
            )
        });

        let proofs = futures::future::try_join_all(proofs).await?;

        let mut stdin = SP1Stdin::new();
        stdin.write(&proofs);
        stdin.write(&header.state_root);

        let proof = self
            .client
            .prove(&self.storage_slots_pk, stdin)
            .mode(self.proof_mode)
            .strategy(self.fulfillment_strategy)
            .await?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_operator_prover_config() {
        assert_eq!(
            parse_fulfillment_strategy("auction").unwrap(),
            FulfillmentStrategy::Auction
        );
        assert_eq!(
            parse_fulfillment_strategy("HOSTED").unwrap(),
            FulfillmentStrategy::Hosted
        );
        assert_eq!(
            parse_fulfillment_strategy("reserved").unwrap(),
            FulfillmentStrategy::Reserved
        );
        assert_eq!(
            network_mode_for(FulfillmentStrategy::Auction),
            NetworkMode::Mainnet
        );
        assert_eq!(
            network_mode_for(FulfillmentStrategy::Reserved),
            NetworkMode::Reserved
        );
        assert_eq!(parse_proof_mode("plonk").unwrap(), SP1ProofMode::Plonk);
        assert_eq!(parse_proof_mode("GROTH16").unwrap(), SP1ProofMode::Groth16);
        assert!(parse_fulfillment_strategy("network").is_err());
        assert!(parse_proof_mode("compressed").is_err());
    }
}
