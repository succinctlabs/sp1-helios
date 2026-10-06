//! Execute recorded inputs and optionally generate a locally verified EVM proof.

use std::path::PathBuf;

use alloy::sol_types::SolType;
use alloy_primitives::{hex, B256};
use anyhow::{ensure, Context, Result};
use clap::{Parser, ValueEnum};
use sp1_helios_primitives::types::{ExecutionHeaderProofOutputs, ProofInputs, ProofOutputs};
use sp1_sdk::{Elf, HashableKey, ProveRequest, Prover, ProverClient, ProvingKey, SP1Stdin};

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Mode {
    LightClient,
    ExecutionHeader,
}

#[derive(Parser)]
struct Args {
    #[arg(long)]
    input: PathBuf,
    #[arg(long, value_enum)]
    mode: Mode,
    /// Output JSON consumed by the real-verifier contract validation profile.
    #[arg(long)]
    output: PathBuf,
    /// Generate a real PLONK proof after successful execution.
    #[arg(long)]
    prove: bool,
    /// Use the local CUDA prover; requires the cuda Cargo feature.
    #[cfg(feature = "cuda")]
    #[arg(long)]
    cuda: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    sp1_sdk::utils::setup_logger();
    #[cfg(feature = "cuda")]
    if args.cuda {
        let client = ProverClient::builder().cuda().build().await;
        return validate(&client, &args).await;
    }
    let client = ProverClient::builder().cpu().build().await;
    validate(&client, &args).await
}

async fn validate<P: Prover>(client: &P, args: &Args) -> Result<()> {
    if let Some(parent) = args
        .output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let cbor = std::fs::read(&args.input)?;
    let inputs: ProofInputs = serde_cbor::from_slice(&cbor)?;
    let elf = match args.mode {
        Mode::LightClient => Elf::Static(include_bytes!("../../elf/light_client")),
        Mode::ExecutionHeader => Elf::Static(include_bytes!("../../elf/execution_header")),
    };
    let mut stdin = SP1Stdin::new();
    stdin.write_slice(&cbor);
    let (public_values, report) = client
        .execute(elf.clone(), stdin.clone())
        .await
        .context("guest execution failed")?;
    ensure!(
        report.exit_code == 0,
        "guest exit code {}",
        report.exit_code
    );
    let values = public_values.to_vec();
    let (previous, target, root, block, hash, receipts) = match args.mode {
        Mode::LightClient => {
            let outputs = ProofOutputs::abi_decode(&values)?;
            (
                outputs.prevHead,
                outputs.newHead,
                outputs.executionStateRoot,
                outputs.executionBlockNumber,
                B256::ZERO,
                B256::ZERO,
            )
        }
        Mode::ExecutionHeader => {
            let outputs = ExecutionHeaderProofOutputs::abi_decode(&values)?;
            (
                outputs.prevHead,
                outputs.newHead,
                outputs.executionStateRoot,
                outputs.executionBlockNumber,
                outputs.executionBlockHash,
                outputs.executionReceiptsRoot,
            )
        }
    };
    ensure!(target > previous, "guest did not advance the head");
    if let Some(header) = &inputs.execution_header {
        ensure!(root == header.state_root, "state root mismatch");
        ensure!(
            block == alloy_primitives::U256::from(header.number),
            "block number mismatch"
        );
        if matches!(args.mode, Mode::ExecutionHeader) {
            ensure!(hash == header.hash_slow(), "block hash mismatch");
            ensure!(receipts == header.receipts_root, "receipts root mismatch");
        }
    }
    println!(
        "executed {:?}: head {previous} -> {target}, block {block}, cycles {}",
        args.mode,
        report.total_instruction_count()
    );
    let pk = client
        .setup(elf)
        .await
        .map_err(|e| anyhow::anyhow!("setup failed: {e}"))?;
    let mut artifact = serde_json::json!({
        "mode": match args.mode { Mode::LightClient => "light-client", Mode::ExecutionHeader => "execution-header" },
        "vkey": pk.verifying_key().bytes32(),
        "publicValues": hex::encode_prefixed(&values),
        "proof": "0x",
        "sp1Version": client.version(),
        "sdkVersion": "6.8.1",
        "cycles": report.total_instruction_count(),
    });
    if args.prove {
        println!("generating real PLONK proof");
        let proof = client
            .prove(&pk, stdin)
            .plonk()
            .await
            .map_err(|e| anyhow::anyhow!("proving failed: {e}"))?;
        client.verify(&proof, pk.verifying_key(), None)?;
        ensure!(
            proof.public_values.to_vec() == values,
            "proof/executor output mismatch"
        );
        proof.save(args.output.with_extension("bin"))?;
        artifact["proof"] = hex::encode_prefixed(proof.bytes()).into();
        println!("real PLONK proof verified with SDK");
    }
    std::fs::write(&args.output, serde_json::to_vec_pretty(&artifact)?)?;
    Ok(())
}
