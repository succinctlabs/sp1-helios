use ethers_core::types::H256;
use helios::{
    common::consensus::types::Update,
    common::consensus::utils,
    consensus::{
        constants,
        rpc::{nimbus_rpc::NimbusRpc, ConsensusRpc},
        Inner,
    },
    prelude::*,
};
use serde::Deserialize;
use ssz_rs::prelude::*;
use std::sync::Arc;
use tokio::sync::{mpsc::channel, watch};
pub mod relay;
// mod types;

pub async fn get_updates(client: &Inner<NimbusRpc>) -> Vec<Update> {
    let period = utils::calc_sync_period(client.store.finalized_header.slot.into());

    let updates = client
        .rpc
        .get_updates(period, constants::MAX_REQUEST_LIGHT_CLIENT_UPDATES)
        .await
        .unwrap();

    updates.clone()
}

pub async fn get_latest_checkpoint() -> H256 {
    let cf = checkpoints::CheckpointFallback::new()
        .build()
        .await
        .unwrap();

    // Fetch the latest mainnet checkpoint
    cf.fetch_latest_checkpoint(&networks::Network::MAINNET)
        .await
        .unwrap()
}

pub async fn get_checkpoint_for_epoch(epoch: u64) -> H256 {
    let rpc = NimbusRpc::new("https://www.lightclientdata.org");
    const SLOTS_PER_EPOCH: u64 = 32;

    let first_slot = epoch * SLOTS_PER_EPOCH;
    let mut block = rpc.get_block(first_slot).await.unwrap();
    H256::from_slice(block.hash_tree_root().unwrap().as_ref())
}

#[derive(Deserialize)]
struct ApiResponse {
    success: bool,
    result: ExecutionStateProof,
}

#[derive(Deserialize, Debug)]
pub struct ExecutionStateProof {
    #[serde(rename = "executionStateRoot")]
    execution_state_root: H256,
    #[serde(rename = "executionStateBranch")]
    execution_state_branch: Vec<H256>,
    gindex: String,
}

pub async fn get_execution_state_root_proof(
    slot: u64,
) -> Result<ExecutionStateProof, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let url = format!(
        "https://beaconapi-git-xavier-get-update.succinct.xyz/api/beacon/proof/lightclient/update/{}",
        slot
    );

    let response: ApiResponse = client.get(url).send().await?.json().await?;

    if response.success {
        Ok(response.result)
    } else {
        Err("API request was not successful".into())
    }
}

pub async fn get_client(checkpoint: Vec<u8>) -> Inner<NimbusRpc> {
    let consensus_rpc = "https://www.lightclientdata.org";

    let base_config = networks::mainnet();
    let config = Config {
        consensus_rpc: consensus_rpc.to_string(),
        execution_rpc: String::new(),
        chain: base_config.chain,
        forks: base_config.forks,
        strict_checkpoint_age: false,
        ..Default::default()
    };

    let (block_send, _) = channel(256);
    let (finalized_block_send, _) = watch::channel(None);
    let (channel_send, _) = watch::channel(None);

    let mut client = Inner::<NimbusRpc>::new(
        consensus_rpc,
        block_send,
        finalized_block_send,
        channel_send,
        Arc::new(config),
    );

    client.bootstrap(&checkpoint).await.unwrap();
    client
}