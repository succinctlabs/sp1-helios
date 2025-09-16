use alloy::network::EthereumWallet;
use alloy::primitives::Address;
use alloy::providers::ProviderBuilder;
use alloy::signers::local::PrivateKeySigner;
use sp1_helios_script::operator::SP1HeliosOperator;
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
}

#[tokio::main]
async fn main() {
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
            .expect("Failed to parse private key"),
    );

    let provider = ProviderBuilder::new()
        .wallet(wallet)
        .connect_http(args.rpc_url.parse().expect("Failed to parse RPC URL"));

    let operator = SP1HeliosOperator::new(
        provider,
        args.contract_address,
        args.source_consensus_rpc,
        args.source_chain_id,
    )
    .await;

    // Run the operator indefinitely, spawns a background task
    tracing::info!("Running operator");
    let handle = operator.run(Duration::from_secs(args.loop_delay_mins * 60));

    tokio::signal::ctrl_c().await.unwrap();

    handle.shutdown().await;
}
