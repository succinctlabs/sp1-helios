use crate::*;
use alloy::providers::{Provider, WalletProvider};
use alloy::transports::Transport;
use alloy::{primitives::Address, sol};
use anyhow::{Context, Result};
use helios_consensus_core::consensus_spec::MainnetConsensusSpec;
use helios_ethereum::consensus::Inner;
use helios_ethereum::rpc::http_rpc::HttpRpc;
use helios_ethereum::rpc::ConsensusRpc;
use log::{error, info};
use sp1_helios_primitives::types::ProofInputs;
use sp1_sdk::{EnvProver, ProverClient, SP1ProofWithPublicValues, SP1ProvingKey, SP1Stdin};
use std::time::Duration;

const ELF: &[u8] = include_bytes!("../../elf/sp1-helios-elf");

sol! {
    #[allow(missing_docs)]
    #[sol(rpc)]
    contract SP1Helios {
        bytes32 public immutable GENESIS_VALIDATORS_ROOT;
        uint256 public immutable GENESIS_TIME;
        uint256 public immutable SECONDS_PER_SLOT;
        uint256 public immutable SLOTS_PER_PERIOD;
        uint32 public immutable SOURCE_CHAIN_ID;
        uint256 public head;
        mapping(uint256 => bytes32) public syncCommittees;
        mapping(uint256 => bytes32) public executionStateRoots;
        mapping(uint256 => bytes32) public headers;
        bytes32 public heliosProgramVkey;
        address public verifier;

        struct ProofOutputs {
            bytes32 executionStateRoot;
            bytes32 newHeader;
            bytes32 nextSyncCommitteeHash;
            uint256 newHead;
            bytes32 prevHeader;
            uint256 prevHead;
            bytes32 syncCommitteeHash;
        }

        event HeadUpdate(uint256 indexed slot, bytes32 indexed root);
        event SyncCommitteeUpdate(uint256 indexed period, bytes32 indexed root);

        function update(bytes calldata proof, bytes calldata publicValues) external;
        function getSyncCommitteePeriod(uint256 slot) internal view returns (uint256);
        function getCurrentSlot() internal view returns (uint256);
        function getCurrentEpoch() internal view returns (uint256);
    }
}

pub struct SP1HeliosOperator<P, T> {
    client: Arc<EnvProver>,
    provider: P,
    pk: Arc<SP1ProvingKey>,
    contract_address: Address,
    _marker: std::marker::PhantomData<T>,
}

impl<T, P> SP1HeliosOperator<P, T>
where
    T: Transport + Clone,
    P: Provider<T> + WalletProvider,
{
    pub async fn new(provider: P, contract_address: Address) -> Self {
        let client = ProverClient::from_env();
        let (pk, _) = client.setup(ELF);

        Self {
            client: Arc::new(client),
            provider,
            pk: Arc::new(pk),
            contract_address,
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

        info!("Updating to new head block: {:?} from {:?}", latest_block, head);

        // Create program inputs
        let expected_current_slot = client.expected_current_slot();
        let inputs = ProofInputs {
            updates,
            finality_update,
            expected_current_slot,
            store: client.store.clone(),
            genesis_root: client.config.chain.genesis_root,
            forks: client.config.forks.clone(),
        };
        let encoded_proof_inputs = serde_cbor::to_vec(&inputs)?;
        stdin.write_slice(&encoded_proof_inputs);

        // Generate proof.
        let proof = tokio::task::spawn_blocking({
            let client = self.client.clone();
            let pk = self.pk.clone();

            move || client.prove(&pk, &stdin).groth16().run()
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

        let receipt = contract
            .update(proof.bytes().into(), proof.public_values.to_vec().into())
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
