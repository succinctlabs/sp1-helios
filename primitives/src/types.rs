use alloy_primitives::B256;
use alloy_sol_types::sol;
use common::config::types::Forks;
use consensus_core::types::{Bytes32, FinalityUpdate, LightClientStore, Update};
use ssz_rs::prelude::*;
pub use ssz_rs::prelude::{Bitvector, Vector};

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct ProofInputs {
    pub updates: Vec<Update>,
    pub finality_update: FinalityUpdate,
    pub expected_current_slot: u64,
    pub store: LightClientStore,
    pub genesis_root: Bytes32,
    pub forks: Forks,
    pub execution_state_proof: ExecutionStateProof,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct ExecutionStateProof {
    #[serde(rename = "executionStateRoot")]
    pub execution_state_root: B256,
    #[serde(rename = "executionStateBranch")]
    pub execution_state_branch: Vec<B256>,
    pub gindex: String,
}

/// bytes32 prevHeader;
/// bytes32 newHeader;
/// bytes32 syncCommitteeHash;
/// bytes32 nextSyncCommitteeHash;
/// uint256 prevHead;
/// uint256 newHead;
/// bytes32 executionStateRoot;
pub type ProofOutputs = sol! {
    tuple(bytes32, bytes32, bytes32, bytes32, uint256, uint256, bytes32)
};
