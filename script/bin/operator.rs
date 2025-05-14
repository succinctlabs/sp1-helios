use alloy::network::EthereumWallet;
use alloy::primitives::Address;
use alloy::providers::ProviderBuilder;
use alloy::signers::local::PrivateKeySigner;
use sp1_helios_script::operator::SP1HeliosOperator;
use std::env;
use std::time::Duration;
#[tokio::main]
async fn main() {
    // todo!(nhtyy): replace with tracing
    env::set_var("RUST_LOG", "info");
    dotenv::dotenv().ok();
    env_logger::init();

    let loop_delay_mins = env::var("LOOP_DELAY_MINS")
        .unwrap_or("5".to_string())
        .parse::<u64>()
        .map(|d| Duration::from_secs(d * 60))
        .expect("Failed to parse LOOP_DELAY_MINS");

    let rpc_url = env::var("DEST_RPC_URL")
        .expect("DEST_RPC_URL not set")
        .parse()
        .expect("Failed to parse DEST_RPC_URL");

    let contract_address: Address = env::var("CONTRACT_ADDRESS")
        .expect("CONTRACT_ADDRESS not set")
        .parse()
        .expect("Failed to parse CONTRACT_ADDRESS");

    let private_key: PrivateKeySigner = env::var("PRIVATE_KEY")
        .expect("PRIVATE_KEY not set")
        .parse()
        .expect("Failed to pase PRIVATE_KEY");

    let wallet = EthereumWallet::from(private_key);

    let provider = ProviderBuilder::new()
        .with_recommended_fillers()
        .wallet(wallet)
        .on_http(rpc_url);

    let operator = SP1HeliosOperator::new(provider, contract_address).await;

    // Run the operator indefinitely.
    operator.run(loop_delay_mins).await
}
