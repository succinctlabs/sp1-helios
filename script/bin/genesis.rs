use alloy::signers::local::PrivateKeySigner;
use alloy_primitives::Address;
use anyhow::Result;
/// Generate genesis parameters for light client contract
use clap::Parser;
use helios_consensus_core::consensus_spec::{ConsensusSpec, MainnetConsensusSpec};
use serde::{Deserialize, Serialize};
use sp1_helios_script::get_client;
use sp1_sdk::{HashableKey, Prover, ProverClient};
use std::{
    fmt::Debug,
    fs,
    path::{Path, PathBuf},
};
use tracing_subscriber::{fmt, EnvFilter};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use tree_hash::TreeHash;

const LIGHT_CLIENT_ELF: &[u8] = include_bytes!("../../elf/light_client");
const STORAGE_ELF: &[u8] = include_bytes!("../../elf/storage");
const SECONDS_PER_SLOT: u64 = 12;

#[derive(Parser, Debug, Clone)]
#[command(about = "Get the genesis parameters from a block.")]
pub struct GenesisArgs {
    /// The optional slot to use for the genesis client.
    #[arg(long)]
    pub slot: Option<u64>,

    /// The RPC URL to deploy the contract too.
    #[arg(long)]
    pub rpc_url: String,

    /// The private key to use for the deployer account.
    ///
    /// Defaults to the anvil private key if ledger is not set.
    #[arg(long, required_unless_present = "ledger")]
    pub private_key: Option<String>,

    /// Whether to use a ledger for deployment.
    #[arg(long, conflicts_with = "private_key")]
    pub ledger: bool,

    /// The ledger derivation path to use for deployment.
    #[arg(
        long,
        default_value = "0",
        requires = "ledger",
        conflicts_with = "private_key"
    )]
    pub ledger_path: usize,

    /// The SP1 verifier address to use for the deployer account.
    #[arg(long)]
    pub sp1_verifier_address: Address,

    /// The chain ID of which were proving the consensus of.
    #[arg(long, default_value = "1")]
    pub source_chain_id: u64,

    /// The RPC URL of the source chain.
    #[arg(long)]
    pub source_consensus_rpc: String,

    /// The Etherscan API key to use for the deployer account.
    #[arg(long)]
    pub etherscan_api_key: Option<String>,

    /// The guardian address that will own the contract.
    ///
    /// If not set, the deployer address will be used as the guardian address.
    #[arg(long)]
    pub guardian_address: Option<Address>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenesisConfig {
    pub execution_state_root: String,
    pub execution_block_number: u64,
    pub genesis_time: u64,
    pub genesis_validators_root: String,
    pub guardian: String,
    pub head: u64,
    pub header: String,
    pub light_client_vkey: String,
    pub seconds_per_slot: u64,
    pub slots_per_epoch: u64,
    pub slots_per_period: u64,
    pub source_chain_id: u64,
    pub storage_slot_vkey: String,
    pub sync_committee_hash: String,
    pub verifier: String,
}

#[tokio::main]
pub async fn main() {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_default()
                .add_directive("genesis=debug".parse().unwrap()),
        )
        .try_init()
        .expect("Failed to initialize tracing");

    panic_if_forge_not_installed();

    let args = GenesisArgs::parse();

    // Compute the Vkeys.
    let client = ProverClient::builder().cpu().build();
    tracing::info!("Setting up light client program...");
    let (_, lightclient_pk) = client.setup(LIGHT_CLIENT_ELF);
    tracing::info!("Setting up storage slots program...");
    let (_, storage_slots_pk) = client.setup(STORAGE_ELF);

    let helios_client = get_client(args.slot, &args.source_consensus_rpc, args.source_chain_id)
        .await
        .expect("Failed to create genesis client");
    let finalized_header = helios_client
        .store
        .finalized_header
        .clone()
        .beacon()
        .tree_hash_root();
    let head = helios_client.store.finalized_header.clone().beacon().slot;

    // Handle an edge-case where we end up on a slot that is not a checkpoint slot.
    assert!(
        head.is_multiple_of(32),
        "Head is not a checkpoint slot, please deploy again."
    );

    let sync_committee_hash: alloy_primitives::FixedBytes<32> = helios_client
        .store
        .current_sync_committee
        .clone()
        .tree_hash_root();
    let genesis_time = helios_client.config.chain.genesis_time;
    let genesis_root = helios_client.config.chain.genesis_root;

    // Get the workspace root with cargo metadata to make the paths.
    let workspace_root = PathBuf::from(
        cargo_metadata::MetadataCommand::new()
            .exec()
            .unwrap()
            .workspace_root,
    );

    // Get the account associated with the private key.
    let deployer_address = get_signer_address(&args).await;

    // Attempt using the GUARDIAN_ADDRESS, otherwise default to the address derived from the private key.
    // If the GUARDIAN_ADDRESS is not set, or is empty, the deployer address is used as the guardian address.
    let guardian = match args.guardian_address {
        Some(guardian_address) => guardian_address,
        None => deployer_address,
    };

    let genesis_config = GenesisConfig {
        execution_state_root: format!(
            "0x{:x}",
            *helios_client
                .store
                .finalized_header
                .execution()
                .expect("Execution payload doesn't exist.")
                .state_root()
        ),
        execution_block_number: *helios_client
            .store
            .finalized_header
            .execution()
            .expect("Execution payload doesn't exist.")
            .block_number(),
        genesis_time,
        genesis_validators_root: format!("0x{genesis_root:x}"),
        guardian: guardian.to_string(),
        head,
        header: format!("0x{finalized_header:x}"),
        light_client_vkey: lightclient_pk.bytes32(),
        storage_slot_vkey: storage_slots_pk.bytes32(),
        seconds_per_slot: SECONDS_PER_SLOT,
        slots_per_epoch: MainnetConsensusSpec::slots_per_epoch(),
        slots_per_period: MainnetConsensusSpec::slots_per_sync_committee_period(),
        source_chain_id: args.source_chain_id,
        sync_committee_hash: format!("0x{sync_committee_hash:x}"),
        verifier: args.sp1_verifier_address.to_string(),
    };

    write_genesis_config(&workspace_root, &genesis_config).expect("Failed to write genesis config");

    deploy_via_forge(&args).expect("Failed to call forge script");
}

/// Find the workspace root.
fn find_project_root() -> Option<PathBuf> {
    let mut path = std::env::current_dir().ok()?;
    while !path.join(".git").exists() {
        if !path.pop() {
            return None;
        }
    }
    Some(path)
}

/// Install the contract dependencies.
fn forge_install() -> Result<()> {
    let project_root = find_project_root().expect("Failed to find project root");
    let mut command = std::process::Command::new("forge");
    command
        .arg("install")
        .current_dir(project_root.join("contracts"));

    let output = command.status()?;

    if !output.success() {
        return Err(anyhow::anyhow!("Forge install failed to run"));
    }

    Ok(())
}

/// Deploy the contract via forge, this will read from the genesis.json file in the contracts directory.
fn deploy_via_forge(args: &GenesisArgs) -> Result<()> {
    forge_install()?;

    let project_root = find_project_root().expect("Failed to find project root");
    let mut command = std::process::Command::new("forge");
    command
        .args([
            "script",
            "script/Deploy.s.sol",
            "--rpc-url",
            &args.rpc_url,
            "--broadcast",
        ])
        .current_dir(project_root.join("contracts"));

    if let Some(ref private_key) = args.private_key {
        assert!(
            private_key.len() == 66,
            "Expecting private key to be of the from 0x..."
        );

        command.arg("--private-key").arg(private_key);
    }

    if args.ledger {
        command.arg("--ledger");
    }

    if let Some(ref etherscan_api_key) = args.etherscan_api_key {
        command.arg("--etherscan-api-key").arg(etherscan_api_key);
    }

    let output = command.status()?;

    if !output.success() {
        return Err(anyhow::anyhow!("Forge script failed to run"));
    }

    Ok(())
}

fn panic_if_forge_not_installed() {
    let output = std::process::Command::new("which")
        .arg("forge")
        .status()
        .expect("failed to run `which forge`");

    if !output.success() {
        panic!("Forge is not installed, please see https://getfoundry.sh/");
    }
}

/// Write the genesis config to the contracts directory.
fn write_genesis_config(workspace_root: &Path, genesis_config: &GenesisConfig) -> Result<()> {
    let genesis_config_path = workspace_root.join("contracts").join("genesis.json");
    fs::write(
        genesis_config_path,
        serde_json::to_string_pretty(&genesis_config)?,
    )?;

    Ok(())
}

async fn get_signer_address(args: &GenesisArgs) -> Address {
    use alloy::signers::ledger::HDPath;

    if args.ledger {
        // Create the signer, with the given HDPath.
        let signer =
            alloy::signers::ledger::LedgerSigner::new(HDPath::LedgerLive(args.ledger_path), None)
                .await
                .expect("Failed to create ledger signer");

        signer
            .get_address()
            .await
            .expect("Failed to get address from ledger signer")
    } else {
        let signer = args
            .private_key
            .as_ref()
            .expect("Private key not set, this should be set by now if !ledger, this is a bug.")
            .parse::<PrivateKeySigner>()
            .expect("Failed to parse private key");

        signer.address()
    }
}
