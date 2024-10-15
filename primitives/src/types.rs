use alloy_primitives::B256;
use alloy_sol_types::sol;
use consensus_core::types::Forks;
use consensus_core::types::{FinalityUpdate, LightClientStore, Update};
use serde::{Deserialize, Serialize};
use ssz_rs::prelude::*;
pub use ssz_rs::prelude::{Bitvector, Vector};

#[derive(Serialize, Deserialize, Debug)]
pub struct ProofInputs {
    pub updates: Vec<Update>,
    pub finality_update: FinalityUpdate,
    pub expected_current_slot: u64,
    pub store: LightClientStore,
    pub genesis_root: B256,
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

sol! {
    struct ProofOutputs {
        bytes32 executionStateRoot;
        bytes32 newHeader;
        bytes32 nextSyncCommitteeHash;
        uint256 newHead;
        bytes32 prevHeader;
        uint256 prevHead;
        bytes32 syncCommitteeHash;
    }
}
