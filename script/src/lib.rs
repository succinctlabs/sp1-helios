use alloy_primitives::B256;
use helios_consensus_core::{
    calc_sync_period,
    consensus_spec::MainnetConsensusSpec,
    types::{BeaconBlock, Update},
};
use helios_ethereum::rpc::ConsensusRpc;
use helios_ethereum::{
    config::{checkpoints, networks::Network, Config},
    consensus::Inner,
    rpc::http_rpc::HttpRpc,
};

use anyhow::{anyhow, Result};
use std::sync::Arc;
use tokio::sync::{mpsc::channel, watch};
use tree_hash::TreeHash;

pub mod handle;
pub mod operator;

pub const MAX_REQUEST_LIGHT_CLIENT_UPDATES: u8 = 128;

/// Fetch updates for client
pub async fn get_updates(
    client: &Inner<MainnetConsensusSpec, HttpRpc>,
) -> Vec<Update<MainnetConsensusSpec>> {
    let period =
        calc_sync_period::<MainnetConsensusSpec>(client.store.finalized_header.beacon().slot);

    let mut updates = client
        .rpc
        .get_updates(period, MAX_REQUEST_LIGHT_CLIENT_UPDATES)
        .await
        .unwrap();

    updates.retain(|update| {
        let signature_period = calc_sync_period::<MainnetConsensusSpec>(*update.signature_slot());
        update_period_in_requested_window(
            signature_period,
            period,
            MAX_REQUEST_LIGHT_CLIENT_UPDATES,
        )
    });
    updates.sort_by_key(|update| *update.signature_slot());
    updates
}

fn update_period_in_requested_window(signature_period: u64, start_period: u64, count: u8) -> bool {
    signature_period >= start_period && signature_period < start_period + u64::from(count)
}

/// Fetch latest checkpoint from chain to bootstrap client to the latest state.
pub async fn get_latest_checkpoint(chain_id: u64) -> Result<B256> {
    let cf = checkpoints::CheckpointFallback::new()
        .build()
        .await
        .map_err(|e| anyhow!("error building checkpoint fallback: {}", e.to_string()))?;

    let network = Network::from_chain_id(chain_id).unwrap_or_else(|_| {
        panic!("unknown network: {chain_id}");
    });

    cf.fetch_latest_checkpoint(&network)
        .await
        .map_err(|e| anyhow!("error fetching latest checkpoint: {}", e.to_string()))
}

/// Setup a client from a checkpoint slot.
///
/// This method will also bootstrap the client to the given slot, or the latest checkpoint if no slot is provided.
pub async fn get_client(
    slot: Option<u64>,
    consensus_rpc: &str,
    chain_id: u64,
) -> Result<Inner<MainnetConsensusSpec, HttpRpc>> {
    let network = Network::from_chain_id(chain_id).unwrap();
    let base_config = network.to_base_config();

    let config = Config {
        consensus_rpc: consensus_rpc.parse()?,
        execution_rpc: None,
        chain: base_config.chain,
        forks: base_config.forks,
        strict_checkpoint_age: false,
        ..Default::default()
    };

    let (block_send, _) = channel(256);
    let (finalized_block_send, _) = watch::channel(None);
    let (channel_send, _) = watch::channel(None);

    let mut client = Inner::<MainnetConsensusSpec, HttpRpc>::new(
        consensus_rpc,
        block_send,
        finalized_block_send,
        channel_send,
        Arc::new(config),
    );

    let root = match slot {
        Some(slot) => {
            let block: BeaconBlock<MainnetConsensusSpec> = client
                .rpc
                .get_block(slot)
                .await
                .map_err(|e| anyhow!("error getting block: {}", e.to_string()))?;

            block.tree_hash_root()
        }
        None => get_latest_checkpoint(chain_id).await?,
    };

    client
        .bootstrap(root)
        .await
        .map_err(|e| anyhow!("error bootstrapping client: {}", e.to_string()))?;

    Ok(client)
}

#[cfg(test)]
mod tests {
    use super::update_period_in_requested_window;

    #[test]
    fn accepts_requested_update_window() {
        assert!(update_period_in_requested_window(1289, 1289, 128));
        assert!(update_period_in_requested_window(1416, 1289, 128));
    }

    #[test]
    fn rejects_out_of_window_update_periods() {
        assert!(!update_period_in_requested_window(1288, 1289, 128));
        assert!(!update_period_in_requested_window(1417, 1289, 128));
    }
}
