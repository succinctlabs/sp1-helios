/// Update the light client once
use helios::consensus::rpc::ConsensusRpc;
use helios_2_script::{get_execution_state_root_proof, get_updates};
use sp1_helios_primitives::types::ProofInputs;
use sp1_sdk::{utils::setup_logger, ProverClient, SP1Stdin};
use tracing::{error, info};

const ELF: &[u8] = include_bytes!("../../program/elf/riscv32im-succinct-zkvm-elf");
use alloy::{
    network::{Ethereum, EthereumWallet},
    primitives::{Address, U256},
    providers::{
        fillers::{ChainIdFiller, FillProvider, GasFiller, JoinFill, NonceFiller, WalletFiller},
        Identity, Provider, ProviderBuilder, RootProvider,
    },
    signers::local::PrivateKeySigner,
    sol,
    transports::http::{Client, Http},
};
use anyhow::Result;
use helios_2_script::*;
use ssz_rs::prelude::*;
use std::sync::Arc;
use std::{env, time::Duration};

type EthereumFillProvider = FillProvider<
    JoinFill<
        JoinFill<JoinFill<JoinFill<Identity, GasFiller>, NonceFiller>, ChainIdFiller>,
        WalletFiller<EthereumWallet>,
    >,
    RootProvider<Http<Client>>,
    Http<Client>,
    Ethereum,
>;

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
            bytes32 syncCommitteeHash;
            bytes32 nextSyncCommitteeHash;
            uint256 prevHead;
            uint256 newHead;
            bytes32 executionStateRoot;
        }

        event HeadUpdate(uint256 indexed slot, bytes32 indexed root);
        event SyncCommitteeUpdate(uint256 indexed period, bytes32 indexed root);

        function update(bytes calldata proof, bytes calldata publicValues) external;
        function getSyncCommitteePeriod(uint256 slot) internal view returns (uint256);
        function getCurrentSlot() internal view returns (uint256);
        function getCurrentEpoch() internal view returns (uint256);
    }
}
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    setup_logger();
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

    let wallet_filler: Arc<EthereumFillProvider> = Arc::new(provider);

    let contract = SP1LightClient::new(contract_address, wallet_filler.clone());
    // Get the current slot from the contract
    let head: u64 = contract
        .head()
        .call()
        .await
        .unwrap()
        .head
        .try_into()
        .unwrap();

    // Get peroid from head
    let period: u64 = contract
        .getSyncCommitteePeriod(U256::from(head))
        .call()
        .await
        .unwrap()
        ._0
        .try_into()
        .unwrap();

    // Fetch the checkpoint at that slot
    let checkpoint = get_checkpoint(head).await;

    // Setup & sync client.
    let mut helios_client = get_client(checkpoint.as_bytes().to_vec()).await;
    let updates = get_updates(&helios_client).await;
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
    if contract_current_sync_committee.to_vec()
        != helios_client
            .store
            .current_sync_committee
            .hash_tree_root()
            .unwrap()
            .as_ref()
    {
        panic!("Client not in sync with contract");
    }
    let (helios_client, updates) = sync_client(
        helios_client,
        updates,
        head,
        contract_current_sync_committee,
        contract_next_sync_committee,
    )
    .await;

    let finality_update = helios_client.rpc.get_finality_update().await.unwrap();
    let latest_block = finality_update.finalized_header.slot;

    if latest_block.as_u64() <= head {
        info!("Contract is up to date. Nothing to update.");
        return Ok(());
    }

    let execution_state_root_proof = get_execution_state_root_proof(latest_block.into())
        .await
        .unwrap();

    let expected_current_slot = helios_client.expected_current_slot();
    let inputs = ProofInputs {
        updates,
        finality_update,
        expected_current_slot,
        store: helios_client.store.clone(),
        genesis_root: helios_client
            .config
            .chain
            .genesis_root
            .clone()
            .try_into()
            .unwrap(),
        forks: helios_client.config.forks.clone(),
        execution_state_proof: execution_state_root_proof,
    };

    let mut stdin = SP1Stdin::new();
    let encoded_inputs = serde_cbor::to_vec(&inputs).unwrap();
    stdin.write_slice(&encoded_inputs);

    // Generate proof.
    let client = ProverClient::new();
    let (pk, _) = client.setup(ELF);
    let proof = client.prove(&pk, stdin).plonk().run().unwrap();

    // Relay the update to the contract
    let proof_as_bytes = if env::var("SP1_PROVER").unwrap().to_lowercase() == "mock" {
        vec![]
    } else {
        proof.bytes()
    };
    let public_values_bytes = proof.public_values.to_vec();

    let gas_limit = relay::get_gas_limit(chain_id);
    let max_fee_per_gas = relay::get_fee_cap(chain_id, wallet_filler.root()).await;

    let nonce = wallet_filler.get_transaction_count(relayer_address).await?;

    const NUM_CONFIRMATIONS: u64 = 3;
    const TIMEOUT_SECONDS: u64 = 60;

    // Relay the update to the contract
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

    if !receipt.status() {
        error!("Transaction reverted!");
    } else {
        info!("Transaction hash: {:?}", receipt.transaction_hash);
    }

    Ok(())
}
