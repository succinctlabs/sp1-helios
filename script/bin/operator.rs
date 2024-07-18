use alloy::{
    network::{Ethereum, EthereumWallet},
    primitives::Address,
    providers::{
        fillers::{ChainIdFiller, FillProvider, GasFiller, JoinFill, NonceFiller, WalletFiller},
        Identity, Provider, ProviderBuilder, RootProvider,
    },
    signers::local::PrivateKeySigner,
    sol,
    transports::http::{Client, Http},
};
use alloy_primitives::{B256, U256};
use anyhow::Result;

use helios::consensus::{rpc::nimbus_rpc::NimbusRpc, Inner};
use helios::{consensus::rpc::ConsensusRpc, types::Update};
use helios_2_script::*;
use log::{error, info};
use sp1_helios_primitives::types::ProofInputs;
use sp1_sdk::{PlonkBn254Proof, ProverClient, SP1ProofWithPublicValues, SP1ProvingKey, SP1Stdin};
use ssz_rs::prelude::*;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use zduny_wasm_timer::SystemTime;
const ELF: &[u8] = include_bytes!("../../program/elf/riscv32im-succinct-zkvm-elf");

/// Alias the fill provider for the Ethereum network. Retrieved from the instantiation of the
/// ProviderBuilder. Recommended method for passing around a ProviderBuilder.
type EthereumFillProvider = FillProvider<
    JoinFill<
        JoinFill<JoinFill<JoinFill<Identity, GasFiller>, NonceFiller>, ChainIdFiller>,
        WalletFiller<EthereumWallet>,
    >,
    RootProvider<Http<Client>>,
    Http<Client>,
    Ethereum,
>;

struct SP1LightClientOperator {
    client: ProverClient,
    pk: SP1ProvingKey,
    wallet_filler: Arc<EthereumFillProvider>,
    contract_address: Address,
    relayer_address: Address,
    chain_id: u64,
}

sol! {
    #[allow(missing_docs)]
    #[sol(rpc)]
    contract SP1LightClient {
        bytes32 public immutable GENESIS_VALIDATORS_ROOT;
        uint256 public immutable GENESIS_TIME;
        uint256 public immutable SECONDS_PER_SLOT;
        uint256 public immutable SLOTS_PER_PERIOD;
        uint32 public immutable SOURCE_CHAIN_ID;
        uint256 public head;
        mapping(uint256 => bytes32) public syncCommittees;
        mapping(uint256 => bytes32) public executionStateRoots;
        mapping(uint256 => bytes32) public headers;
        bytes32 public telepathyProgramVkey;
        address public verifier;

        struct ProofOutputs {
            bytes32 prevHeader;
            bytes32 newHeader;
            bytes32 prevSyncCommitteeHash;
            bytes32 newSyncCommitteeHash;
            uint256 prevHead;
            uint256 newHead;
        }

        event HeadUpdate(uint256 indexed slot, bytes32 indexed root);
        event SyncCommitteeUpdate(uint256 indexed period, bytes32 indexed root);

        function update(bytes calldata proof, bytes calldata publicValues) external;
        function getSyncCommitteePeriod(uint256 slot) internal view returns (uint256);
        function getCurrentSlot() internal view returns (uint256);
        function getCurrentEpoch() internal view returns (uint256);
    }
}
impl SP1LightClientOperator {
    pub async fn new() -> Self {
        dotenv::dotenv().ok();

        let client = ProverClient::new();
        let (pk, _) = client.setup(ELF);
        let chain_id: u64 = env::var("CHAIN_ID")
            .expect("CHAIN_ID not set")
            .parse()
            .unwrap();
        let rpc_url = env::var("RPC_URL")
            .expect("RPC_URL not set")
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
        let provider = ProviderBuilder::new()
            .with_recommended_fillers()
            .wallet(wallet)
            .on_http(rpc_url);

        Self {
            client,
            pk,
            wallet_filler: Arc::new(provider),
            chain_id,
            contract_address,
            relayer_address,
        }
    }

    async fn sync_client(
        &self,
        mut client: Inner<NimbusRpc>,
        mut updates: Vec<Update>,
    ) -> (Inner<NimbusRpc>, Vec<Update>) {
        let contract = SP1LightClient::new(self.contract_address, self.wallet_filler.clone());

        // Fetch required values
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
        let contract_current_sync_committee = contract
            .syncCommittees(U256::from(period))
            .call()
            .await
            .unwrap()
            ._0;
        let contract_next_sync_committee = contract
            .syncCommittees(U256::from(period + 1))
            .call()
            .await
            .unwrap()
            ._0;

        // Sync client with contract
        if contract_current_sync_committee.to_vec()
            != client
                .store
                .current_sync_committee
                .hash_tree_root()
                .unwrap()
                .as_ref()
        {
            panic!("Client not in sync with contract");
        }

        // Helios' bootstrap does not set next_sync_committee (see implementation).
        // If the contract has this value, we need to catch up helios.
        // The first update is the catch-up update.
        let contract_has_next_sync_committee =
            contract_next_sync_committee.to_vec() != B256::ZERO.to_vec();
        let client_has_next_sync_committee = client.store.next_sync_committee.is_some();
        if contract_has_next_sync_committee && !client_has_next_sync_committee {
            if let Some(mut first_update) = updates.first().cloned() {
                // Sanity check: update will catch-up client with contract
                assert_eq!(
                    first_update
                        .next_sync_committee
                        .hash_tree_root()
                        .unwrap()
                        .as_ref(),
                    contract_next_sync_committee.to_vec()
                );

                updates.remove(0);
                match client.verify_update(&first_update) {
                    Ok(_) => {
                        client.apply_update(&first_update);

                        // Sanity check: client is caught up with contract
                        assert_eq!(
                            client
                                .store
                                .clone()
                                .next_sync_committee
                                .unwrap()
                                .hash_tree_root()
                                .unwrap()
                                .as_ref(),
                            contract_next_sync_committee.to_vec()
                        );
                        assert_eq!(head, client.store.finalized_header.slot.as_u64());
                    }
                    Err(e) => {
                        panic!("Failed to verify catch-up update: {:?}", e);
                    }
                }
            } else {
                panic!("No catch-up updates available");
            }
        }

        (client, updates)
    }

    async fn request_update(
        &self,
        client: Inner<NimbusRpc>,
    ) -> Result<Option<SP1ProofWithPublicValues>> {
        let contract = SP1LightClient::new(self.contract_address, self.wallet_filler.clone());
        let head: u64 = contract.head().call().await?.head.try_into().unwrap();

        let mut stdin = SP1Stdin::new();

        let updates = get_updates(&client).await;
        let (client, updates) = self.sync_client(client, updates).await;

        let now = SystemTime::now();
        let finality_update = client.rpc.get_finality_update().await.unwrap();
        let latest_block = finality_update.finalized_header.slot;

        if latest_block.as_u64() <= head {
            info!("Contract is up to date. Nothing to update.");
            return Ok(None);
        }

        let execution_state_proof = get_execution_state_root_proof(latest_block.into())
            .await
            .unwrap();

        let inputs = ProofInputs {
            updates,
            finality_update,
            now,
            genesis_time: client.config.chain.genesis_time,
            store: client.store,
            genesis_root: client.config.chain.genesis_root.clone(),
            forks: client.config.forks.clone(),
            execution_state_proof,
        };

        let encoded_proof_inputs = serde_cbor::to_vec(&inputs)?;
        stdin.write_slice(&encoded_proof_inputs);

        let proof = self.client.prove(&self.pk, stdin).plonk().run().unwrap();

        info!("New head: {:?}", latest_block.as_u64());
        Ok(Some(proof))
    }

    /// Relay an update proof to the SP1 LightClient contract.
    async fn relay_update(&self, proof: SP1ProofWithPublicValues) -> Result<()> {
        // TODO: sp1_sdk should return empty bytes in mock mode.
        let proof_as_bytes = if env::var("SP1_PROVER").unwrap().to_lowercase() == "mock" {
            vec![]
        } else {
            proof.bytes()
            // Strip the 0x prefix from proof_str, if it exists.
        };
        let public_values_bytes = proof.public_values.to_vec();

        let contract = SP1LightClient::new(self.contract_address, self.wallet_filler.clone());

        let gas_limit = relay::get_gas_limit(self.chain_id);
        let max_fee_per_gas = relay::get_fee_cap(self.chain_id, self.wallet_filler.root()).await;

        let nonce = self
            .wallet_filler
            .get_transaction_count(self.relayer_address)
            .await?;

        // Wait for 3 required confirmations with a timeout of 60 seconds.
        const NUM_CONFIRMATIONS: u64 = 3;
        const TIMEOUT_SECONDS: u64 = 60;
        let receipt = contract
            .update(proof_as_bytes.into(), public_values_bytes.into())
            .gas_price(max_fee_per_gas)
            .gas(gas_limit)
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
        }

        info!("Transaction hash: {:?}", receipt.transaction_hash);

        Ok(())
    }

    async fn run(&mut self, loop_delay_mins: u64) -> Result<()> {
        info!("Starting SP1 Telepathy operator");

        loop {
            let contract = SP1LightClient::new(self.contract_address, self.wallet_filler.clone());

            // Get the current slot from the contract
            let slot = contract.head().call().await?.head.try_into().unwrap();

            // Fetch the checkpoint at that slot
            let checkpoint = get_checkpoint(slot).await;

            // Get the client from the checkpoint
            let client = get_client(checkpoint.as_bytes().to_vec()).await;

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
async fn main() {
    env::set_var("RUST_LOG", "info");
    dotenv::dotenv().ok();
    env_logger::init();

    let loop_delay_mins_env = env::var("LOOP_DELAY_MINS");
    let mut loop_delay_mins = 5;
    if loop_delay_mins_env.is_ok() {
        loop_delay_mins = loop_delay_mins_env
            .unwrap()
            .parse::<u64>()
            .expect("invalid LOOP_DELAY_MINS");
    }

    let update_delay_blocks_env = env::var("UPDATE_DELAY_BLOCKS");
    let mut update_delay_blocks = 300;
    if update_delay_blocks_env.is_ok() {
        update_delay_blocks = update_delay_blocks_env
            .unwrap()
            .parse::<u64>()
            .expect("invalid UPDATE_DELAY_BLOCKS");
    }

    let data_commitment_max_env = env::var("DATA_COMMITMENT_MAX");
    // Note: This default value reflects the max data commitment size that can be rquested from the
    // Celestia node.
    let mut data_commitment_max = 1000;
    if data_commitment_max_env.is_ok() {
        data_commitment_max = data_commitment_max_env
            .unwrap()
            .parse::<u64>()
            .expect("invalid DATA_COMMITMENT_MAX");
    }

    let mut operator = SP1LightClientOperator::new().await;
    loop {
        if let Err(e) = operator.run(loop_delay_mins).await {
            error!("Error running operator: {}", e);
        }
    }
}
