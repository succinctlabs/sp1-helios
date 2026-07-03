use alloy::network::EthereumWallet;
use alloy::primitives::Address;
use alloy::providers::ProviderBuilder;
use alloy::signers::local::PrivateKeySigner;
use anyhow::{Context, Result};
use sp1_helios_script::operator::{
    parse_fulfillment_strategy, parse_proof_mode, ExecutionCommitment, ProverSettings,
    SP1HeliosOperator,
};
use sp1_sdk::network::signer::NetworkSigner;
use std::env;
use std::time::Duration;
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(about = "Run the SP1 Helios operator.")]
pub struct OperatorArgs {
    /// The RPC URL where the light client contract is deployed.
    #[arg(long)]
    pub rpc_url: String,

    /// The address of the light client contract.
    #[arg(long)]
    pub contract_address: Address,

    /// The chain ID of the source chain.
    #[arg(long)]
    pub source_chain_id: u64,

    /// The RPC URL of the source chain.
    #[arg(long)]
    pub source_consensus_rpc: String,

    #[arg(long)]
    pub private_key: String,

    /// The delay between operator runs in minutes.
    #[arg(long, default_value = "5")]
    pub loop_delay_mins: u64,

    /// Commit finalized execution block hash and receipts root in addition to the state root.
    #[arg(long, default_value_t = false)]
    pub commit_execution_header: bool,
}

fn required_env(key: &str) -> Result<String> {
    env::var(key).with_context(|| format!("{key} is not set"))
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_default()
                .add_directive("operator=debug".parse().unwrap()),
        )
        .try_init()
        .expect("Failed to initialize tracing");

    let args = OperatorArgs::parse();

    let wallet = EthereumWallet::from(
        args.private_key
            .parse::<PrivateKeySigner>()
            .context("failed to parse destination transaction private key")?,
    );

    let provider = ProviderBuilder::new().wallet(wallet).connect_http(
        args.rpc_url
            .parse()
            .context("failed to parse destination RPC URL")?,
    );

    let execution_commitment = if args.commit_execution_header {
        ExecutionCommitment::HeaderWithReceipts
    } else {
        ExecutionCommitment::StateRootOnly
    };
    let fulfillment_strategy =
        parse_fulfillment_strategy(&env_or("SP1_HELIOS_FULFILLMENT_STRATEGY", "auction"))?;
    let proof_mode = parse_proof_mode(&env_or("SP1_HELIOS_PROOF_MODE", "plonk"))?;
    let use_kms_requester = env::var("USE_KMS_REQUESTER")
        .unwrap_or_else(|_| "false".to_string())
        .parse::<bool>()
        .context("failed to parse USE_KMS_REQUESTER as bool")?;
    let network_private_key = required_env("NETWORK_PRIVATE_KEY")?;
    let network_signer = if use_kms_requester {
        NetworkSigner::aws_kms(&network_private_key)
            .await
            .context("failed to create AWS KMS Prover Network signer")?
    } else {
        NetworkSigner::local(&network_private_key)
            .context("failed to create local Prover Network signer")?
    };
    let prover_settings = ProverSettings {
        network_signer,
        fulfillment_strategy,
        proof_mode,
    };

    let operator = SP1HeliosOperator::new(
        provider,
        args.contract_address,
        args.source_consensus_rpc,
        args.source_chain_id,
        execution_commitment,
        prover_settings,
    )
    .await;

    // Run the operator indefinitely, spawns a background task
    tracing::info!("Running operator");
    let handle = operator.run(Duration::from_secs(args.loop_delay_mins * 60));

    tokio::signal::ctrl_c().await.unwrap();

    handle.shutdown().await;

    Ok(())
}
