use ethers::{
    providers::{Http, Middleware, Provider},
    types::U256,
};

/// Get the gas limit associated with the chain id.
pub fn get_gas_limit(chain_id: u64) -> U256 {
    match chain_id {
        42161 | 421614 => U256::from(25_000_000),
        _ => U256::from(1_500_000),
    }
}

/// Get the gas fee cap associated with the chain id, using the provider to get the gas price.
pub async fn get_fee_cap(chain_id: u64, provider: &Provider<Http>) -> U256 {
    // Base percentage multiplier for the gas fee.
    let multiplier =
        if chain_id == 17000 || chain_id == 421614 || chain_id == 11155111 || chain_id == 84532 {
            100
        } else {
            20
        };

    // Get the gas price.
    let gas_price = provider.get_gas_price().await.unwrap();

    // Calculate the fee cap.
    gas_price * (100 + multiplier) / 100
}
