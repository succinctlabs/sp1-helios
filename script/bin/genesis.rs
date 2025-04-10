use alloy::signers::local::PrivateKeySigner;
use alloy_primitives::Address;
use anyhow::Result;
/// Generate genesis parameters for light client contract
use clap::Parser;
use serde::{Deserialize, Serialize};
use sp1_helios_script::{get_checkpoint, get_client, get_latest_checkpoint};
use sp1_sdk::{utils, HashableKey, Prover, ProverClient};
use std::{
    env, fs,
    path::{Path, PathBuf},
};
use tree_hash::TreeHash;

const HELIOS_ELF: &[u8] = include_bytes!("../../elf/sp1-helios-elf");

#[derive(Parser, Debug, Clone)]
#[command(about = "Get the genesis parameters from a block.")]
pub struct GenesisArgs {
    #[arg(long)]
    pub slot: Option<u64>,
    #[arg(long, default_value = ".env")]
    pub env_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenesisConfig {
    pub execution_state_root: String,
    pub genesis_time: u64,
    pub genesis_validators_root: String,
    pub guardian: String,
    pub head: u64,
    pub header: String,
    pub helios_program_vkey: String,
    pub seconds_per_slot: u64,
    pub slots_per_epoch: u64,
    pub slots_per_period: u64,
    pub source_chain_id: u64,
    pub sync_committee_hash: String,
    pub verifier: String,
}

#[tokio::main]
pub async fn main() -> Result<()> {
    utils::setup_logger();

    let args = GenesisArgs::parse();

    // This fetches the .env file from the project root. If the command is invoked in the contracts/ directory,
    // the .env file in the root of the repo is used.
    if let Some(root) = find_project_root() {
        dotenv::from_path(root.join(args.env_file)).ok();
    } else {
        eprintln!(
            "Warning: Could not find project root. {} file not loaded.",
            args.env_file
        );
    }

    let client = ProverClient::builder().cpu().build();
    let (_pk, vk) = client.setup(HELIOS_ELF);

    let checkpoint;
    if let Some(temp_slot) = args.slot {
        checkpoint = get_checkpoint(temp_slot).await?;
    } else {
        checkpoint = get_latest_checkpoint().await;
    }
    let sp1_prover = env::var("SP1_PROVER").unwrap();

    let mut verifier = Address::ZERO;
    if sp1_prover != "mock" {
        verifier = env::var("SP1_VERIFIER_ADDRESS").unwrap().parse().unwrap();
    }

    let helios_client = get_client(checkpoint).await?;
    let finalized_header = helios_client
        .store
        .finalized_header
        .clone()
        .beacon()
        .tree_hash_root();
    let head = helios_client.store.finalized_header.clone().beacon().slot;
    let sync_committee_hash = helios_client
        .store
        .current_sync_committee
        .clone()
        .tree_hash_root();
    let genesis_time = helios_client.config.chain.genesis_time;
    let genesis_root = helios_client.config.chain.genesis_root;
    const SECONDS_PER_SLOT: u64 = 12;
    const SLOTS_PER_EPOCH: u64 = 32;
    const SLOTS_PER_PERIOD: u64 = SLOTS_PER_EPOCH * 256;
    let source_chain_id: u64 = match env::var("SOURCE_CHAIN_ID") {
        Ok(val) => val.parse().unwrap(),
        Err(_) => {
            println!("SOURCE_CHAIN_ID not set, defaulting to mainnet");
            1 // Mainnet chain ID
        }
    };

    // Get the workspace root with cargo metadata to make the paths.
    let workspace_root = PathBuf::from(
        cargo_metadata::MetadataCommand::new()
            .exec()
            .unwrap()
            .workspace_root,
    );

    // Read the Genesis config from the contracts directory.
    let mut genesis_config = get_existing_genesis_config(&workspace_root)?;

    genesis_config.genesis_validators_root = format!("0x{:x}", genesis_root);
    genesis_config.genesis_time = genesis_time;
    genesis_config.seconds_per_slot = SECONDS_PER_SLOT;
    genesis_config.slots_per_period = SLOTS_PER_PERIOD;
    genesis_config.slots_per_epoch = SLOTS_PER_EPOCH;
    genesis_config.source_chain_id = source_chain_id;
    genesis_config.sync_committee_hash = format!("0x{:x}", sync_committee_hash);
    genesis_config.header = format!("0x{:x}", finalized_header);
    genesis_config.execution_state_root = format!(
        "0x{:x}",
        helios_client
            .store
            .finalized_header
            .execution()
            .expect("Execution payload doesn't exist.")
            .state_root()
    );
    genesis_config.head = head;
    genesis_config.helios_program_vkey = vk.bytes32();
    genesis_config.verifier = format!("0x{:x}", verifier);

    // Get the account associated with the private key.
    let private_key = env::var("PRIVATE_KEY").unwrap();
    let signer: PrivateKeySigner = private_key.parse().expect("Failed to parse private key");
    let deployer_address = signer.address();

    // Attempt using the GUARDIAN_ADDRESS, otherwise default to the address derived from the private key.
    // If the GUARDIAN_ADDRESS is not set, or is empty, the deployer address is used as the guardian address.
    let guardian = match env::var("GUARDIAN_ADDRESS") {
        Ok(guardian_addr) if !guardian_addr.is_empty() => guardian_addr,
        _ => format!("0x{:x}", deployer_address),
    };

    genesis_config.guardian = guardian;

    write_genesis_config(&workspace_root, &genesis_config)?;

    Ok(())
}

fn find_project_root() -> Option<PathBuf> {
    let mut path = std::env::current_dir().ok()?;
    while !path.join(".git").exists() {
        if !path.pop() {
            return None;
        }
    }
    Some(path)
}

/// Get the existing genesis config from the contracts directory.
fn get_existing_genesis_config(workspace_root: &Path) -> Result<GenesisConfig> {
    let genesis_config_path = workspace_root.join("contracts").join("genesis.json");
    let genesis_config_content = std::fs::read_to_string(genesis_config_path)?;
    let genesis_config: GenesisConfig = serde_json::from_str(&genesis_config_content)?;
    Ok(genesis_config)
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
