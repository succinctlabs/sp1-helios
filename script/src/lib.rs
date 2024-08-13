use alloy_primitives::B256;
use ethers_core::types::H256;
use helios::{
    config::networks::Network,
    consensus::{
        constants,
        rpc::{nimbus_rpc::NimbusRpc, ConsensusRpc},
        Inner,
    },
    consensus_core::{types::Update, utils},
    prelude::*,
};
use serde::Deserialize;
use sp1_helios_primitives::types::ExecutionStateProof;
use ssz_rs::prelude::*;
use std::sync::Arc;
use tokio::sync::{mpsc::channel, watch};
pub mod relay;

/// Fetch updates for client
pub async fn get_updates(client: &Inner<NimbusRpc>) -> Vec<Update> {
    let period = utils::calc_sync_period(client.store.finalized_header.slot.into());

    let updates = client
        .rpc
        .get_updates(period, constants::MAX_REQUEST_LIGHT_CLIENT_UPDATES)
        .await
        .unwrap();

    updates.clone()
}

/// Fetch latest checkpoint from chain to bootstrap client to the latest state.
pub async fn get_latest_checkpoint() -> H256 {
    let cf = checkpoints::CheckpointFallback::new()
        .build()
        .await
        .unwrap();
    
    let chain_id = std::env::var("SOURCE_CHAIN_ID").unwrap();
    let network = Network::from_chain_id(chain_id.parse().unwrap()).unwrap();

    cf.fetch_latest_checkpoint(&network)
        .await
        .unwrap()
}

/// Fetch checkpoint from a slot number.
pub async fn get_checkpoint(slot: u64) -> H256 {
    let rpc_url = std::env::var("SOURCE_CONSENSUS_RPC_URL").unwrap();
    let rpc: NimbusRpc = NimbusRpc::new(&rpc_url);

    let mut block = rpc.get_block(slot).await.unwrap();
    H256::from_slice(block.hash_tree_root().unwrap().as_ref())
}

#[derive(Deserialize)]
struct ApiResponse {
    success: bool,
    result: ExecutionStateProof,
}

/// Fetch merkle proof for the execution state root of a specific slot.
pub async fn get_execution_state_root_proof(
    slot: u64,
) -> Result<ExecutionStateProof, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    
    let chain_id = std::env::var("SOURCE_CHAIN_ID").unwrap();
    let url_suffix = match chain_id.as_str() {
        "11155111" => "-sepolia",  // Sepolia chain ID
        "17000" => "-holesky",     // Holesky chain ID
        "1" => "",                 // Mainnet chain ID
        _ => return Err(format!("Unsupported chain ID: {}", chain_id).into()),
    };

    let url = format!(
        "https://beaconapi{}.succinct.xyz/api/beacon/proof/executionStateRoot/{}",
        url_suffix, slot
    );

    let response: ApiResponse = client.get(url).send().await?.json().await?;

    if response.success {
        Ok(response.result)
    } else {
        Err("API request was not successful".into())
    }
}

/// Setup a client from a checkpoint.
pub async fn get_client(checkpoint: Vec<u8>) -> Inner<NimbusRpc> {
    let consensus_rpc = std::env::var("SOURCE_CONSENSUS_RPC_URL").unwrap();
    let chain_id = std::env::var("SOURCE_CHAIN_ID").unwrap();
    let network = Network::from_chain_id(chain_id.parse().unwrap()).unwrap();
    let base_config = network.to_base_config();

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
        &consensus_rpc,
        block_send,
        finalized_block_send,
        channel_send,
        Arc::new(config),
    );
   
    client.bootstrap(&checkpoint).await.unwrap();
    client
}

/// The client from get_client() may be an update behind,
/// so this function catches up the local client to the on-chain contract.
pub async fn sync_client(
    mut client: Inner<NimbusRpc>,
    mut updates: Vec<Update>,
    head: u64,
    contract_current_sync_committee: B256,
    contract_next_sync_committee: B256,
) -> (Inner<NimbusRpc>, Vec<Update>) {
    // Sync client with contract
    if contract_current_sync_committee.to_vec()
        != client
            .store
            .current_sync_committee
            .hash_tree_root()
            .unwrap()
            .as_ref()
    {
        panic!("Client not in sync with contract");
    }

    // Helios' bootstrap does not set next_sync_committee (see implementation).
    // If the contract has this value, we need to catch up helios.
    // The first update is the catch-up update.
    let contract_has_next_sync_committee =
        contract_next_sync_committee.to_vec() != B256::ZERO.to_vec();
    let client_has_next_sync_committee = client.store.next_sync_committee.is_some();
    if contract_has_next_sync_committee && !client_has_next_sync_committee {
        if let Some(mut first_update) = updates.first().cloned() {
            // Sanity check: update will catch-up client with contract
            assert_eq!(
                first_update
                    .next_sync_committee
                    .hash_tree_root()
                    .unwrap()
                    .as_ref(),
                contract_next_sync_committee.to_vec()
            );

            updates.remove(0);
            match client.verify_update(&first_update) {
                Ok(_) => {
                    client.apply_update(&first_update);

                    // Sanity check: client is caught up with contract
                    assert_eq!(
                        client
                            .store
                            .clone()
                            .next_sync_committee
                            .unwrap()
                            .hash_tree_root()
                            .unwrap()
                            .as_ref(),
                        contract_next_sync_committee.to_vec()
                    );
                }
                Err(e) => {
                    panic!("Failed to verify catch-up update: {:?}", e);
                }
            }
        } else {
            panic!("No catch-up updates available");
        }
    }

    (client, updates)
}