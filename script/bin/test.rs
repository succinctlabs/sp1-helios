use alloy_primitives::B256;
use anyhow::Result;
use clap::{command, Parser};
use helios_ethereum::rpc::ConsensusRpc;
use sp1_helios_primitives::types::ProofInputs;
use sp1_helios_script::{get_checkpoint, get_client, get_latest_checkpoint, get_updates};
use sp1_sdk::{utils::setup_logger, ProverClient, SP1Stdin};

const ELF: &[u8] = include_bytes!("../../elf/sp1-helios-elf");
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

    // Get the latest checkpoint.
    let slot = match args.slot {
        Some(s) => s,
        None => {
            // Get latest checkpoint and use it to determine current slot
            let latest_checkpoint = get_latest_checkpoint().await;
            let client = get_client(latest_checkpoint).await?;
            client.expected_current_slot()
        }
    };

    // Find a valid checkpoint by searching backwards from the slot
    let mut checkpoint = B256::ZERO;
    let mut checkpoint_slot: u64 = 0;
    for i in slot.saturating_sub(8000)..slot {
        if let Ok(cp) = get_checkpoint(i).await {
            if get_client(cp).await.is_ok() {
                checkpoint = cp;
                checkpoint_slot = i;
                break;
            }
        }
    }

    // Setup client with the found checkpoint
    let helios_client = get_client(checkpoint).await?;

    let updates = get_updates(&helios_client).await;
    let finality_update = helios_client.rpc.get_finality_update().await.unwrap();
    let expected_current_slot = helios_client.expected_current_slot();

    println!("Checkpoint slot: {}", checkpoint_slot);
    println!(
        "Finality update slot: {}",
        finality_update.attested_header().beacon().slot
    );
    println!(
        "Finality update signature slot: {}",
        finality_update.signature_slot()
    );

    let inputs = ProofInputs {
        updates,
        finality_update,
        expected_current_slot,
        store: helios_client.store.clone(),
        genesis_root: helios_client.config.chain.genesis_root,
        forks: helios_client.config.forks.clone(),
    };

    // Write the inputs to the VM
    let mut stdin = SP1Stdin::new();
    stdin.write_slice(&serde_cbor::to_vec(&inputs)?);

    let prover_client = ProverClient::builder().cpu().build();
    let (_, report) = prover_client
        .execute(ELF, &stdin)
        .calculate_gas(false)
        .run()?;
    println!("Execution Report: {:?}", report);

    Ok(())
}
