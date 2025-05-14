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

pub mod operator;

pub const MAX_REQUEST_LIGHT_CLIENT_UPDATES: u8 = 128;

/// Fetch updates for client
pub async fn get_updates(
    client: &Inner<MainnetConsensusSpec, HttpRpc>,
) -> Vec<Update<MainnetConsensusSpec>> {
    let period =
        calc_sync_period::<MainnetConsensusSpec>(client.store.finalized_header.beacon().slot);

    let updates = client
        .rpc
        .get_updates(period, MAX_REQUEST_LIGHT_CLIENT_UPDATES)
        .await
        .unwrap();

    updates.clone()
}

/// Fetch latest checkpoint from chain to bootstrap client to the latest state.
pub async fn get_latest_checkpoint() -> Result<B256> {
    let cf = checkpoints::CheckpointFallback::new()
        .build()
        .await
        .map_err(|e| anyhow!("error building checkpoint fallback: {}", e.to_string()))?;

    let chain_id = std::env::var("SOURCE_CHAIN_ID").expect("SOURCE_CHAIN_ID not set");
    let network = Network::from_chain_id(chain_id.parse().expect("Can parse SOURCE_CHAIN_ID"))
        .unwrap_or_else(|_| {
            panic!("unknown network: {}", chain_id);
        });

    cf.fetch_latest_checkpoint(&network)
        .await
        .map_err(|e| anyhow!("error fetching latest checkpoint: {}", e.to_string()))
}

/// Setup a client from a checkpoint slot.
///
/// This method will also bootstrap the client to the given slot, or the latest checkpoint if no slot is provided.
pub async fn get_client(slot: Option<u64>) -> Result<Inner<MainnetConsensusSpec, HttpRpc>> {
    let consensus_rpc = std::env::var("SOURCE_CONSENSUS_RPC_URL").unwrap();
    let chain_id = std::env::var("SOURCE_CHAIN_ID").unwrap();
    let network = Network::from_chain_id(chain_id.parse().unwrap()).unwrap();
    let base_config = network.to_base_config();

    let config = Config {
        consensus_rpc: consensus_rpc.to_string(),
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
        &consensus_rpc,
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
        None => get_latest_checkpoint().await?,
    };

    client
        .bootstrap(root)
        .await
        .map_err(|e| anyhow!("error bootstrapping client: {}", e.to_string()))?;

    Ok(client)
}
