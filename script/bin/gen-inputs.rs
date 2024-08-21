/// Generate cbor-encoded inputs for the program. For external use, not used in sp1-helios itsef.
use anyhow::Result;
use clap::Parser;
use helios::consensus::rpc::ConsensusRpc;
use sp1_helios_script::*;
use sp1_helios_script::{get_execution_state_root_proof, get_updates};
use sp1_helios_primitives::types::ProofInputs;
use sp1_sdk::utils::setup_logger;
use ssz_rs::prelude::*;
use std::fs::File;
use std::io::Write;
use tracing::error;

#[derive(Parser, Debug, Clone)]
#[command(about = "Get the genesis parameters from a block.")]
pub struct GenesisArgs {
    #[arg(long)]
    pub slot: Option<u64>,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    setup_logger();
    let args = GenesisArgs::parse();

    // Get the current slot from the contract or fetch the latest checkpoint
    let checkpoint = if let Some(slot) = args.slot {
        get_checkpoint(slot).await
    } else {
        get_latest_checkpoint().await
    };

    // Setup client.
    let helios_client = get_client(checkpoint.to_vec()).await;
    let updates = get_updates(&helios_client).await;
    let finality_update = helios_client.rpc.get_finality_update().await.unwrap();
    let latest_block = finality_update.finalized_header.slot;

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

    let file_path = "examples/input.cbor";

    // Serialize inputs to a CBOR-encoded vector
    let cbor_data = match serde_cbor::to_vec(&inputs) {
        Ok(data) => data,
        Err(e) => {
            error!("Failed to serialize inputs to CBOR: {}", e);
            return Err(e.into());
        }
    };

    // Write the CBOR-encoded vector to a file
    let mut file = File::create(file_path)?;
    file.write_all(&cbor_data)?;

    Ok(())
}
