use anyhow::Result;
use clap::{command, Parser};
use helios_ethereum::rpc::ConsensusRpc;
use sp1_helios_primitives::types::ProofInputs;
use sp1_helios_script::{
    get_checkpoint, get_client, get_execution_state_root_proof, get_latest_checkpoint, get_updates,
};
use sp1_sdk::{utils::setup_logger, ProverClient, SP1Stdin};

const ELF: &[u8] = include_bytes!("../../elf/riscv32im-succinct-zkvm-elf");
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
    let helios_client = get_client(checkpoint).await;
    let updates = get_updates(&helios_client).await;
    let finality_update = helios_client.rpc.get_finality_update().await.unwrap();
    let latest_block = finality_update.finalized_header.beacon().slot;

    let execution_state_root_proof = get_execution_state_root_proof(latest_block).await.unwrap();

    let expected_current_slot = helios_client.expected_current_slot();
    let inputs = ProofInputs {
        updates,
        finality_update,
        expected_current_slot,
        store: helios_client.store.clone(),
        genesis_root: helios_client.config.chain.genesis_root,
        forks: helios_client.config.forks.clone(),
        execution_state_proof: execution_state_root_proof,
    };

    // Write the inputs to the VM
    let mut stdin = SP1Stdin::new();
    stdin.write_slice(&serde_cbor::to_vec(&inputs)?);

    let prover_client = ProverClient::new();
    let (_, report) = prover_client.execute(ELF, stdin).run()?;
    println!("Execution Report: {:?}", report);

    Ok(())
}
