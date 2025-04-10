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
pub async fn get_latest_checkpoint() -> B256 {
    let cf = checkpoints::CheckpointFallback::new()
        .build()
        .await
        .unwrap();

    let chain_id = std::env::var("SOURCE_CHAIN_ID").expect("SOURCE_CHAIN_ID not set");
    let network = Network::from_chain_id(chain_id.parse().unwrap()).unwrap();

    cf.fetch_latest_checkpoint(&network).await.unwrap()
}

/// Fetch checkpoint from a slot number.
pub async fn get_checkpoint(slot: u64) -> Result<B256> {
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
    let client = Inner::<MainnetConsensusSpec, HttpRpc>::new(
        &consensus_rpc,
        block_send,
        finalized_block_send,
        channel_send,
        Arc::new(config),
    );

    let block: BeaconBlock<MainnetConsensusSpec> = client
        .rpc
        .get_block(slot)
        .await
        .map_err(|e| anyhow!("error getting block: {}", e.to_string()))?;

    Ok(B256::from_slice(block.tree_hash_root().as_ref()))
}

/// Setup a client from a checkpoint.
pub async fn get_client(checkpoint: B256) -> Result<Inner<MainnetConsensusSpec, HttpRpc>> {
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

    let mut client = Inner::new(
        &consensus_rpc,
        block_send,
        finalized_block_send,
        channel_send,
        Arc::new(config),
    );

    client
        .bootstrap(checkpoint)
        .await
        .map_err(|e| anyhow!("error bootstrapping client: {}", e.to_string()))?;
    Ok(client)
}
