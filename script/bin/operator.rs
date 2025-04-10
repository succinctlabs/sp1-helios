use alloy::providers::Provider;
use alloy::{
    network::EthereumWallet, primitives::Address, providers::ProviderBuilder,
    signers::local::PrivateKeySigner, sol,
};
use alloy_primitives::{B256, U256};
use anyhow::Result;
use helios_consensus_core::consensus_spec::MainnetConsensusSpec;
use helios_ethereum::consensus::Inner;
use helios_ethereum::rpc::http_rpc::HttpRpc;
use helios_ethereum::rpc::ConsensusRpc;
use log::{error, info};
use reqwest::Url;
use sp1_helios_primitives::types::ProofInputs;
use sp1_helios_script::*;
use sp1_sdk::{EnvProver, ProverClient, SP1ProofWithPublicValues, SP1ProvingKey, SP1Stdin};
use std::env;
use std::time::Duration;
use tree_hash::TreeHash;

const ELF: &[u8] = include_bytes!("../../elf/sp1-helios-elf");

struct SP1HeliosOperator {
    client: EnvProver,
    pk: SP1ProvingKey,
    wallet: EthereumWallet,
    rpc_url: Url,
    contract_address: Address,
    relayer_address: Address,
}

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

impl SP1HeliosOperator {
    pub async fn new() -> Self {
        dotenv::dotenv().ok();

        let client = ProverClient::from_env();
        let (pk, _) = client.setup(ELF);
        let rpc_url = env::var("DEST_RPC_URL")
            .expect("DEST_RPC_URL not set")
            .parse()
            .unwrap();

        let private_key = env::var("PRIVATE_KEY").expect("PRIVATE_KEY not set");
        let contract_address: Address = env::var("CONTRACT_ADDRESS")
            .expect("CONTRACT_ADDRESS not set")
            .parse()
            .unwrap();
        let signer: PrivateKeySigner = private_key.parse().expect("Failed to parse private key");
        let relayer_address = signer.address();
        let wallet = EthereumWallet::from(signer);

        Self {
            client,
            pk,
            wallet,
            rpc_url,
            contract_address,
            relayer_address,
        }
    }

    /// Fetch values and generate an 'update' proof for the SP1 Helios contract.
    async fn request_update(
        &self,
        mut client: Inner<MainnetConsensusSpec, HttpRpc>,
    ) -> Result<Option<SP1ProofWithPublicValues>> {
        // Fetch required values.
        let provider = ProviderBuilder::new().on_http(self.rpc_url.clone());
        let contract = SP1Helios::new(self.contract_address, provider);
        let head: u64 = contract
            .head()
            .call()
            .await
            .unwrap()
            .head
            .try_into()
            .unwrap();
        let period: u64 = contract
            .getSyncCommitteePeriod(U256::from(head))
            .call()
            .await
            .unwrap()
            ._0
            .try_into()
            .unwrap();
        let contract_next_sync_committee = contract
            .syncCommittees(U256::from(period + 1))
            .call()
            .await
            .unwrap()
            ._0;

        let mut stdin = SP1Stdin::new();

        // Setup client.
        let mut updates = get_updates(&client).await;
        let finality_update = client.rpc.get_finality_update().await.unwrap();

        // Check if contract is up to date
        let latest_block = finality_update.finalized_header().beacon().slot;
        if latest_block <= head {
            info!("Contract is up to date. Nothing to update.");
            return Ok(None);
        }

        // Optimization:
        // Skip processing update inside program if next_sync_committee is already stored in contract.
        // We must still apply the update locally to "sync" the helios client, this is due to
        // next_sync_committee not being stored when the helios client is bootstrapped.
        if !updates.is_empty() {
            let next_sync_committee =
                B256::from_slice(updates[0].next_sync_committee().tree_hash_root().as_ref());

            if contract_next_sync_committee == next_sync_committee {
                println!("Applying optimization, skipping update");
                let temp_update = updates.remove(0);

                client.verify_update(&temp_update).unwrap(); // Panics if not valid
                client.apply_update(&temp_update);
            }
        }

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
        let proof = self.client.prove(&self.pk, &stdin).groth16().run()?;

        info!("Attempting to update to new head block: {:?}", latest_block);
        Ok(Some(proof))
    }

    /// Relay an update proof to the SP1 Helios contract.
    async fn relay_update(&self, proof: SP1ProofWithPublicValues) -> Result<()> {
        let public_values_bytes = proof.public_values.to_vec();

        let wallet_filler = ProviderBuilder::new()
            .with_recommended_fillers()
            .wallet(self.wallet.clone())
            .on_http(self.rpc_url.clone());
        let contract = SP1Helios::new(self.contract_address, wallet_filler.clone());

        let nonce = wallet_filler
            .get_transaction_count(self.relayer_address)
            .await?;

        // Wait for 3 required confirmations with a timeout of 60 seconds.
        const NUM_CONFIRMATIONS: u64 = 3;
        const TIMEOUT_SECONDS: u64 = 60;
        let receipt = contract
            .update(proof.bytes().into(), public_values_bytes.into())
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

    /// Start the operator.
    async fn run(&mut self, loop_delay_mins: u64) -> Result<()> {
        info!("Starting SP1 Helios operator");

        loop {
            let provider = ProviderBuilder::new().on_http(self.rpc_url.clone());
            let contract = SP1Helios::new(self.contract_address, provider);

            // Get the current slot from the contract
            let slot = contract
                .head()
                .call()
                .await
                .unwrap_or_else(|e| {
                    panic!("Failed to get head. Are you sure the SP1Helios is deployed to address: {:?}? Error: {:?}", self.contract_address, e)
                })
                .head
                .try_into()
                .unwrap();

            // Fetch the checkpoint at that slot
            let checkpoint = get_checkpoint(slot).await?;

            // Get the client from the checkpoint
            let client = get_client(checkpoint).await?;

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
            };

            info!("Sleeping for {:?} minutes", loop_delay_mins);
            tokio::time::sleep(tokio::time::Duration::from_secs(60 * loop_delay_mins)).await;
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env::set_var("RUST_LOG", "info");
    dotenv::dotenv().ok();
    env_logger::init();

    let loop_delay_mins = env::var("LOOP_DELAY_MINS")
        .unwrap_or("5".to_string())
        .parse()?;

    let mut operator = SP1HeliosOperator::new().await;
    loop {
        if let Err(e) = operator.run(loop_delay_mins).await {
            error!("Error running operator: {}", e);
        }
    }
}
