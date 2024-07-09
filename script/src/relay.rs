use alloy::providers::{Provider, RootProvider};
use alloy::transports::http::{Client, Http};

/// Get the gas limit associated with the chain id. Note: These values have been found through
/// trial and error and can be configured.
pub fn get_gas_limit(chain_id: u64) -> u128 {
    if chain_id == 42161 || chain_id == 421614 {
        25_000_000
    } else {
        1_500_000
    }
}

/// Get the gas fee cap associated with the chain id, using the provider to get the gas price. Note:
/// These values have been found through trial and error and can be configured.
pub async fn get_fee_cap(chain_id: u64, provider: &RootProvider<Http<Client>>) -> u128 {
    // Base percentage multiplier for the gas fee.
    let mut multiplier = 20;

    // Double the estimated gas fee cap for the testnets.
    if chain_id == 17000 || chain_id == 421614 || chain_id == 11155111 || chain_id == 84532 {
        multiplier = 100
    }

    // Get the gas price.
    let gas_price = provider.get_gas_price().await.unwrap();

    // Calculate the fee cap.
    (gas_price * (100 + multiplier)) / 100
}
