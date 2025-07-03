#![allow(clippy::too_many_arguments)]

use alloy_primitives::{Address, Bytes, B256, U256};
use alloy_sol_types::sol;
use alloy_trie::TrieAccount;
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
    pub contract_storage: Vec<ContractStorage>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ContractStorage {
    pub address: Address,
    pub value: TrieAccount,
    /// The proof that this contracts storage root is correct
    pub mpt_proof: Vec<Bytes>,
    /// The storage slots that we want to prove
    pub storage_slots: Vec<StorageSlotWithProof>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct StorageSlotWithProof {
    pub key: B256,
    pub value: U256,
    /// The proof that this storage slot is correct
    pub mpt_proof: Vec<Bytes>,
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
        /// The execution block number.
        uint256 executionBlockNumber;
        /// The sync committee hash of the current period.
        bytes32 syncCommitteeHash;
        /// The sync committee hash of the next period.
        bytes32 nextSyncCommitteeHash;
        /// Attested storage slots for the given block.
        StorageSlot[] storageSlots;
    }

    struct StorageProofOutputs {
        bytes32 stateRoot;
        StorageSlot[] storageSlots;
    }

    struct StorageSlot {
        bytes32 key;
        bytes32 value;
        address contractAddress;
    }

    #[allow(missing_docs)]
    #[sol(rpc)]
    contract SP1Helios {
        bytes32 public immutable GENESIS_VALIDATORS_ROOT;
        uint256 public immutable GENESIS_TIME;
        uint256 public immutable SECONDS_PER_SLOT;
        uint256 public immutable SLOTS_PER_PERIOD;
        uint32 public immutable SOURCE_CHAIN_ID;
        uint256 public head;
        mapping(uint256 => bytes32) public syncCommittees;
        mapping(uint256 => bytes32) public executionStateRoots;
        mapping(uint256 => bytes32) public headers;
        /// @notice The verification key for the SP1 Helios light client program.
        bytes32 public lightClientVkey;

        /// @notice The verification key for the storage slot proof program.
        bytes32 public storageSlotVkey;
        address public verifier;

        event HeadUpdate(uint256 indexed slot, bytes32 indexed root);
        event SyncCommitteeUpdate(uint256 indexed period, bytes32 indexed root);

        function update(
            bytes calldata proof,
            uint256 newHead,
            bytes32 newHeader,
            bytes32 executionStateRoot,
            uint256 _executionBlockNumber,
            bytes32 syncCommitteeHash,
            bytes32 nextSyncCommitteeHash,
            StorageSlot[] memory _storageSlots
        ) external;

        function getSyncCommitteePeriod(uint256 slot) internal view returns (uint256);
        function getCurrentSlot() internal view returns (uint256);
        function getCurrentEpoch() internal view returns (uint256);
    }
}
