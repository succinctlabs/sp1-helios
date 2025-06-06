use crate::*;
use alloy::providers::{Provider, WalletProvider};
use alloy::sol_types::SolType;
use alloy::transports::Transport;
use alloy::primitives::Address;
use anyhow::{Context, Result};
use helios_consensus_core::consensus_spec::MainnetConsensusSpec;
use helios_ethereum::consensus::Inner;
use helios_ethereum::rpc::http_rpc::HttpRpc;
use helios_ethereum::rpc::ConsensusRpc;
use log::{error, info};
use sp1_helios_primitives::types::{
    ContractStorage, ProofInputs, ProofOutputs, StorageSlotWithProof, SP1Helios,
};
use sp1_helios_primitives::verify_storage_slot_proofs;
use sp1_sdk::{EnvProver, ProverClient, SP1ProofWithPublicValues, SP1ProvingKey, SP1Stdin};
use std::time::Duration;

use tokio::sync::watch;

const ELF: &[u8] = include_bytes!("../../elf/sp1-helios-elf");

pub struct SP1HeliosOperator<P, T> {
    client: Arc<EnvProver>,
    provider: P,
    pk: Arc<SP1ProvingKey>,
    contract_address: Address,
    storage_slots_to_fetch: Option<watch::Receiver<Vec<StorageSlotConfig>>>,
    _marker: std::marker::PhantomData<T>,
}

pub struct StorageSlotConfig {
    pub contract: Address,
    pub keys: Vec<B256>,
}

impl<T, P> SP1HeliosOperator<P, T>
where
    T: Transport + Clone,
    P: Provider<T> + WalletProvider,
{
    pub async fn new(
        provider: P,
        contract_address: Address,
        storage_slot_config: Option<watch::Receiver<Vec<StorageSlotConfig>>>,
    ) -> Self {
        let client = ProverClient::from_env();
        let (pk, _) = client.setup(ELF);

        Self {
            client: Arc::new(client),
            provider,
            pk: Arc::new(pk),
            contract_address,
            storage_slots_to_fetch: storage_slot_config,
            _marker: std::marker::PhantomData,
        }
    }

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
            .head
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
        } else if latest_block % 32 > 0 {
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

        let contract_storage = if let Some(rx) = &self.storage_slots_to_fetch {
            self.get_storage_slots(rx.borrow().as_slice(), *latest_execution_block_number)
                .await?
        } else {
            vec![]
        };

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
            let pk = self.pk.clone();

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

        let po = ProofOutputs::abi_decode(proof.public_values.as_slice(), true)?;

        let tx = contract
            .update(
                proof.bytes().into(),
                po.newHead,
                po.newHeader,
                po.executionStateRoot,
                po.executionBlockNumber,
                po.syncCommitteeHash,
                po.nextSyncCommitteeHash,
                po.storageSlots,
            );

        println!("calldata: {:?}", tx.calldata());

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

    async fn get_storage_slots(
        &self,
        config: &[StorageSlotConfig],
        block_number: u64,
    ) -> Result<Vec<ContractStorage>> {
        let Some(block) = self
            .provider
            .get_block(
                block_number.into(),
                alloy::rpc::types::BlockTransactionsKind::Hashes,
            )
            .await?
        else {
            anyhow::bail!("Failed to get block {block_number} from provider, this was expected to valid since the store claimed to have this block finalized.");
        };

        let futs = config.iter().map(|c| async {
            let proof = self
                .provider
                .get_proof(c.contract, c.keys.clone())
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

            verify_storage_slot_proofs(block.header.state_root, &contract_storage)
                .context(format!("Preflight storage slot proofs failed to verify for contract {:?}", c.contract))?;

            Result::<_, anyhow::Error>::Ok(contract_storage)
        });

        futures::future::try_join_all(futs).await
    }
}

impl<T, P> SP1HeliosOperator<P, T>
where
    P: Provider<T> + WalletProvider,
    T: Transport + Clone,
{
    pub async fn run_once(&self) -> Result<()> {
        let contract = SP1Helios::new(self.contract_address, &self.provider);

        // Get the current slot from the contract
        let slot = contract
            .head()
            .call()
            .await
            .context("Failed to get head from contract")?
            .head
            .try_into()
            .expect("Failed to convert head to u64, this is a bug.");

        // Fetch the checkpoint at that slot
        let client = get_client(Some(slot)).await?;

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

    /// Start the operator, running indefinitely and retrying on failure.
    pub async fn run(&self, loop_delay: Duration) {
        info!("Starting SP1 Helios operator");

        // Make sure we cant hang indefinitely if something goes terribly wrong.
        const TIMEOUT: Duration = Duration::from_secs(60 * 15);

        loop {
            tokio::select! {
                res = self.run_once() => {
                    if let Err(e) = res {
                        error!("Error running operator: {}", e);
                    }
                }
                _ = tokio::time::sleep(TIMEOUT) => {
                    error!("Operator timed out after {:?}", TIMEOUT);
                }
            }

            info!("Sleeping for {:?} minutes", loop_delay.as_secs() / 60);
            tokio::time::sleep(loop_delay).await;
        }
    }
}
