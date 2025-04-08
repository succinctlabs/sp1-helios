use alloy_primitives::B256;
use alloy_sol_types::sol;
use helios_consensus_core::consensus_spec::MainnetConsensusSpec;
use helios_consensus_core::types::Forks;
use helios_consensus_core::types::{FinalityUpdate, LightClientStore, Update};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct ProofInputs {
    pub updates: Vec<Update<MainnetConsensusSpec>>,
    pub finality_update: FinalityUpdate<MainnetConsensusSpec>,
    pub expected_current_slot: u64,
    pub store: LightClientStore<MainnetConsensusSpec>,
    pub genesis_root: B256,
    pub forks: Forks,
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
        /// The previous beacon block header hash.
        bytes32 prevHeader;
        /// The slot of the previous head.
        uint256 prevHead;
        /// The anchor sync committee hash which was used to verify the proof.
        bytes32 prevSyncCommitteeHash;
        /// The slot of the new head.
        uint256 newHead;
        /// The new beacon block header hash.
        bytes32 newHeader;
        /// The execution state root from the execution payload of the new beacon block.
        bytes32 executionStateRoot;
        /// The sync committee hash of the current period.
        bytes32 syncCommitteeHash;
        /// The sync committee hash of the next period.
        bytes32 nextSyncCommitteeHash;
    }
}
