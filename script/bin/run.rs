//! A simple script to generate and verify the proof of a given program.

use ethers_core::types::H256;
use helios::{
    common::consensus::types::Update,
    common::consensus::utils,
    consensus::{
        constants,
        rpc::{nimbus_rpc::NimbusRpc, ConsensusRpc},
        Inner,
    },
    prelude::*,
};
use helios_2_script::{get_execution_state_root_proof, get_updates};
use sp1_helios_primitives::types::{ExecutionStateProof, ProofInputs, ProofOutputs};
use sp1_sdk::{utils::setup_logger, ProverClient, SP1Stdin};
use tracing::{debug, error, info, warn};
use zduny_wasm_timer::SystemTime;
const ELF: &[u8] = include_bytes!("../../program/elf/riscv32im-succinct-zkvm-elf");
use alloy::{
    network::{Ethereum, EthereumWallet},
    primitives::Address,
    providers::{
        fillers::{ChainIdFiller, FillProvider, GasFiller, JoinFill, NonceFiller, WalletFiller},
        Identity, Provider, ProviderBuilder, RootProvider,
    },
    signers::local::PrivateKeySigner,
    sol,
    transports::http::{reqwest::Url, Client, Http},
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
        bytes32 public finalizedHeader;
        bytes32 public syncCommitteeHash;
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
    // Get the current epoch from the contract
    let epoch: u64 = contract
        .getCurrentEpoch()
        .call()
        .await?
        ._0
        .try_into()
        .unwrap();

    // Fetch the checkpoint at that epoch
    let checkpoint = get_checkpoint_for_epoch(epoch).await;

    // Get the client from the checkpoint
    let helios_client = get_client(checkpoint.as_bytes().to_vec()).await;

    let updates = get_updates(&helios_client).await;
    let now = SystemTime::now();
    let finality_update = helios_client.rpc.get_finality_update().await.unwrap();
    let latest_block = finality_update.finalized_header.slot;
    println!("latest block: {:?}", latest_block);
    let execution_state_root_proof = get_execution_state_root_proof(latest_block.into())
        .await
        .unwrap();
    println!(
        "Execution state root proof: {:?}",
        execution_state_root_proof
    );

    let inputs = ProofInputs {
        updates,
        finality_update,
        now,
        genesis_time: helios_client.config.chain.genesis_time,
        store: helios_client.store,
        genesis_root: helios_client.config.chain.genesis_root.clone(),
        forks: helios_client.config.forks.clone(),
        execution_state_proof: execution_state_root_proof,
    };

    let mut stdin = SP1Stdin::new();
    let encoded_inputs = serde_cbor::to_vec(&inputs).unwrap();
    stdin.write_slice(&encoded_inputs);

    let client = ProverClient::new();
    let (pk, _) = client.setup(ELF);
    let proof = client.prove_plonk(&pk, stdin).expect("proving failed");

    // Relay the update to the contract
    let proof_as_bytes = if env::var("SP1_PROVER").unwrap().to_lowercase() == "mock" {
        vec![]
    } else {
        let proof_str = proof.bytes();
        hex::decode(proof_str.replace("0x", "")).unwrap()
    };
    let public_values_bytes = proof.public_values.to_vec();

    let gas_limit = relay::get_gas_limit(chain_id);
    let max_fee_per_gas = relay::get_fee_cap(chain_id, wallet_filler.root()).await;

    let nonce = wallet_filler.get_transaction_count(relayer_address).await?;

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

    if !receipt.status() {
        error!("Transaction reverted!");
    } else {
        info!("Transaction hash: {:?}", receipt.transaction_hash);
    }

    Ok(())
}
