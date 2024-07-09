use alloy_sol_types::{sol, SolStruct, SolValue};
use ssz_rs::prelude::*;
use std::time::SystemTime;

use common::config::types::Forks;
use common::consensus::types::{FinalityUpdate, LightClientStore, Update};
pub use ssz_rs::prelude::{Bitvector, Vector};

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct ProofInputs {
    pub updates: Vec<Update>,
    pub finality_update: FinalityUpdate,
    pub now: SystemTime,
    pub genesis_time: u64,
    pub store: LightClientStore,
    pub genesis_root: Vec<u8>,
    pub forks: Forks,
}

/// bytes32 prevHeader;
/// bytes32 newHeader;
/// bytes32 prevSyncCommitteeHash;
/// bytes32 newSyncCommitteeHash;
/// uin64 prevHead;
/// uin64 newHead;
pub type ProofOutputs = sol! {
    tuple(bytes32, bytes32, bytes32, bytes32, uint256, uint256)
};
