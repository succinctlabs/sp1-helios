//! A simple program to be proven inside the zkVM.

#![no_main]
sp1_zkvm::entrypoint!(main);
// use helios_prover_primitives::types::{
//     BLSPubKey, Bytes32, Header, SignatureBytes, SyncAggregate, SyncCommittee, Vector, U64,
// };

// use ethers::types::H256;
// use eyre::Result;
// use helios::{
//     client,
//     consensus::{
//         self, constants,
//         rpc::{nimbus_rpc::NimbusRpc, ConsensusRpc},
//         types::{Bootstrap, Update},
//         utils, Inner,
//     },
//     prelude::*,
// };
// use std::sync::Arc;
// use tokio::sync::{mpsc::channel, watch};
pub fn main() {
    // let checkpoint: H256 = sp1_zkvm::io::read::<H256>();
    // let mut bootstrap: Bootstrap = sp1_zkvm::io::read::<Bootstrap>();

    // let client = get_client(checkpoint.as_bytes().to_vec(), &mut bootstrap);
    println!("LFG");
}

// fn get_client(checkpoint: Vec<u8>, bootstrap: &mut Bootstrap) -> Inner<NimbusRpc> {
//     let consensus_rpc = "https://www.lightclientdata.org";

//     let base_config = networks::mainnet();
//     let config = Config {
//         consensus_rpc: consensus_rpc.to_string(),
//         //consensus_rpc: String::new(),
//         //execution_rpc: untrusted_rpc_url.to_string(),
//         execution_rpc: String::new(),
//         chain: base_config.chain,
//         forks: base_config.forks,
//         strict_checkpoint_age: false,
//         ..Default::default()
//     };

//     //let check = get_latest_checkpoint().await;
//     //let checkpoint = check.as_bytes().to_vec();
//     //let checkpoint =
//     //hex::decode("60b0473910c8236cdd467f5115ea612f65dd71e052533a60f3864eee0702aaf0").unwrap();

//     let (block_send, _) = channel(256);
//     let (finalized_block_send, _) = watch::channel(None);
//     let (channel_send, _) = watch::channel(None);

//     let mut client = Inner::<NimbusRpc>::new(
//         //"testdata/",
//         consensus_rpc,
//         block_send,
//         finalized_block_send,
//         channel_send,
//         Arc::new(config),
//     );

//     client.bootstrap_from(&checkpoint, bootstrap).unwrap();

//     client
// }
