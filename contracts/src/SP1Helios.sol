// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.22;

import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";

/// @notice Represents a storage slot in an Ethereum smart contract
struct StorageSlot {
    bytes32 key;
    bytes32 value;
    address contractAddress;
}

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

struct StorageSlotProofOutputs {
    bytes32 storageRoot;
    StorageSlot[] storageSlots;
}

struct InitParams {
    bytes32 executionStateRoot;
    uint256 executionBlockNumber;
    uint256 genesisTime;
    bytes32 genesisValidatorsRoot;
    address guardian;
    uint256 head;
    bytes32 header;
    bytes32 lightClientVkey;
    bytes32 storageSlotVkey;
    uint256 secondsPerSlot;
    uint256 slotsPerEpoch;
    uint256 slotsPerPeriod;
    uint256 sourceChainId;
    bytes32 syncCommitteeHash;
    address verifier;
}

/// @title SP1Helios
/// @notice An Ethereum beacon chain light client, built with SP1 and Helios.
contract SP1Helios {
    bytes32 public immutable GENESIS_VALIDATORS_ROOT;
    uint256 public immutable GENESIS_TIME;
    uint256 public immutable SECONDS_PER_SLOT;
    uint256 public immutable SLOTS_PER_PERIOD;
    uint256 public immutable SLOTS_PER_EPOCH;
    uint256 public immutable SOURCE_CHAIN_ID;

    modifier onlyGuardian() {
        require(msg.sender == guardian, "Caller is not the guardian");
        _;
    }

    /// @notice The latest slot the light client has a finalized header for.
    uint256 public head = 0;

    /// @notice The latest execution block number the light client has a finalized execution state root for.
    uint256 public executionBlockNumber = 0;

    /// @notice Maps from a slot to a beacon block header root.
    mapping(uint256 => bytes32) public headers;

    /// @notice Maps from a slot to the current finalized ethereum1 execution state root.
    mapping(uint256 => bytes32) public executionStateRoots;

    /// @notice Maps from a period to the hash for the sync committee.
    mapping(uint256 => bytes32) public syncCommittees;

    /// @notice A mapping from keccak256([abi.encode(blockNumber) || abi.encode(contractAddress) || abi.encode(key)])
    /// @notice to the storage slot value.
    mapping(bytes32 => bytes32) public storageSlots;

    /// @notice The verification key for the SP1 Helios light client program.
    bytes32 public lightClientVkey;

    /// @notice The verification key for the storage slot proof program.
    bytes32 public storageSlotVkey;

    /// @notice The deployed SP1 verifier contract.
    address public verifier;

    /// @notice The address of the guardian
    address public guardian;

    /// @notice Semantic version.
    /// @custom:semver v1.1.0
    string public constant version = "v1.1.0";

    event HeadUpdate(uint256 indexed slot, bytes32 indexed root);
    event SyncCommitteeUpdate(uint256 indexed period, bytes32 indexed root);
    event GuardianUpdate(address indexed newGuardian);
    event GuardianRelinquished();
    event LightClientVkeyUpdate(bytes32 indexed newVkey);
    event StorageSlotVkeyUpdate(bytes32 indexed newVkey);

    error SlotBehindHead(uint256 slot);
    error SyncCommitteeStartMismatch(bytes32 given, bytes32 expected);
    error SyncCommitteeNotSet(uint256 period);
    error NextSyncCommitteeMismatch(bytes32 given, bytes32 expected);
    error NonCheckpointSlot(uint256 slot);
    error MissingStateRoot(uint256 blockNumber);

    constructor(InitParams memory params) {
        GENESIS_VALIDATORS_ROOT = params.genesisValidatorsRoot;
        GENESIS_TIME = params.genesisTime;
        SECONDS_PER_SLOT = params.secondsPerSlot;
        SLOTS_PER_PERIOD = params.slotsPerPeriod;
        SLOTS_PER_EPOCH = params.slotsPerEpoch;
        SOURCE_CHAIN_ID = params.sourceChainId;
        syncCommittees[getSyncCommitteePeriod(params.head)] = params.syncCommitteeHash;
        lightClientVkey = params.lightClientVkey;
        storageSlotVkey = params.storageSlotVkey;
        headers[params.head] = params.header;
        executionStateRoots[params.executionBlockNumber] = params.executionStateRoot;
        executionBlockNumber = params.executionBlockNumber;
        head = params.head;
        verifier = params.verifier;
        guardian = params.guardian;
    }

    /// @notice Updates the light client with a new header, execution state root, and sync committee (if changed)
    /// @param proof The proof bytes for the SP1 proof.
    /// @param newHead The slot of the new head.
    /// @param newHeader The new beacon block header hash.
    /// @param executionStateRoot The execution state root from the execution payload of the new beacon block.
    /// @param _executionBlockNumber The execution block number.
    /// @param syncCommitteeHash The sync committee hash of the current period.
    /// @param nextSyncCommitteeHash The sync committee hash of the next period.
    function update(
        bytes calldata proof,
        uint256 newHead,
        bytes32 newHeader,
        bytes32 executionStateRoot,
        uint256 _executionBlockNumber,
        bytes32 syncCommitteeHash,
        bytes32 nextSyncCommitteeHash,
        StorageSlot[] memory _storageSlots
    ) external {
        // The sync committee for the current head should always be set.
        bytes32 currentSyncCommitteeHash = syncCommittees[getSyncCommitteePeriod(head)];
        if (currentSyncCommitteeHash == bytes32(0)) {
            revert SyncCommitteeNotSet(getSyncCommitteePeriod(head));
        }

        // Fill in the proof outputs with our expected values known by the contract
        // instead of explicity comparing against them, the proof will not verify if they arent correct.
        ProofOutputs memory po = ProofOutputs({
            prevHeader: headers[head],
            prevHead: head,
            prevSyncCommitteeHash: currentSyncCommitteeHash,
            newHead: newHead,
            newHeader: newHeader,
            executionStateRoot: executionStateRoot,
            executionBlockNumber: _executionBlockNumber,
            syncCommitteeHash: syncCommitteeHash,
            nextSyncCommitteeHash: nextSyncCommitteeHash,
            storageSlots: _storageSlots
        });

        // Verify the proof with the associated public values. This will revert if the proof is invalid.
        ISP1Verifier(verifier).verifyProof(lightClientVkey, abi.encode(po), proof);

        // Confirm that the new slot is greater than the current head.
        if (po.newHead <= head) {
            revert SlotBehindHead(po.newHead);
        }

        // Confirm that the new slot is a checkpoint slot.
        // This is useful if the there were ever some delay greater than 30 minutes between updates,
        // as CL nodes typically only store checkpoint slot proofs.
        //
        // This condition is actually checked by the proof, but we include it here for clarity.
        if (po.newHead % 32 != 0) {
            revert NonCheckpointSlot(po.newHead);
        }

        // Update the new CL information.
        head = po.newHead;
        headers[po.newHead] = po.newHeader;

        // Update the EL information.
        executionBlockNumber = po.executionBlockNumber;
        executionStateRoots[po.executionBlockNumber] = po.executionStateRoot;

        // Get the new period associated with the new head.
        uint256 newPeriod = getSyncCommitteePeriod(po.newHead);

        // Set the sync committee for the new period if it is not set.
        // This can happen if the light client was very behind and had a lot of updates.
        // Note: Only the latest sync committee is stored, not the intermediate ones from every update.
        if (syncCommittees[newPeriod] == bytes32(0)) {
            syncCommittees[newPeriod] = po.syncCommitteeHash;
            emit SyncCommitteeUpdate(newPeriod, po.syncCommitteeHash);
        }

        // Set the next sync committee if it is defined and not set.
        if (po.nextSyncCommitteeHash != bytes32(0)) {
            uint256 nextPeriod = newPeriod + 1;

            bytes32 _nextSyncCommitteeHash = syncCommittees[nextPeriod];
            if (_nextSyncCommitteeHash == bytes32(0)) {
                // If the next sync committee is not set, set it.
                syncCommittees[nextPeriod] = po.nextSyncCommitteeHash;
                emit SyncCommitteeUpdate(nextPeriod, po.nextSyncCommitteeHash);
            } else if (_nextSyncCommitteeHash != po.nextSyncCommitteeHash) {
                // If the next sync committee is non-zero, it should match the expected value.
                revert NextSyncCommitteeMismatch(nextSyncCommitteeHash, po.nextSyncCommitteeHash);
            }
        }

        // Set all the storage slots.
        for (uint256 i = 0; i < po.storageSlots.length; i++) {
            bytes32 key = computeStorageSlotKey(
                po.executionBlockNumber, po.storageSlots[i].contractAddress, po.storageSlots[i].key
            );
            storageSlots[key] = po.storageSlots[i].value;
        }

        emit HeadUpdate(po.newHead, po.newHeader);
    }

    /// @notice Verifies a storage slot proof, and saves the storage slots to the contract.
    /// @dev Panics if the proof is invalid.
    /// @param proof The proof bytes for the SP1 proof.
    /// @param _storageSlots The storage slots to verify.
    /// @param blockNumber The block number of the storage slot.
    function updateStorageSlot(
        bytes calldata proof,
        StorageSlot[] memory _storageSlots,
        uint256 blockNumber
    ) external {
        // Verify the proof with the associated public values. This will revert if the proof is invalid.
        verifyStorageSlotsProof(proof, _storageSlots, blockNumber);

        // Set all the storage slots.
        for (uint256 i = 0; i < _storageSlots.length; i++) {
            bytes32 key = computeStorageSlotKey(
                blockNumber, _storageSlots[i].contractAddress, _storageSlots[i].key
            );
            storageSlots[key] = _storageSlots[i].value;
        }
    }

    /// @notice Verifies a storage slot proof.
    /// @dev Panics if the proof is invalid.
    /// @param proof The proof bytes for the SP1 proof.
    /// @param _storageSlots The storage slots to verify.
    /// @param blockNumber The block number of the storage slot.
    function verifyStorageSlotsProof(
        bytes calldata proof,
        StorageSlot[] memory _storageSlots,
        uint256 blockNumber
    ) public view {
        bytes32 executionStateRoot = executionStateRoots[blockNumber];
        if (executionStateRoot == bytes32(0)) {
            revert MissingStateRoot(blockNumber);
        }

        // Fill in the proof outputs with our expected values known by the contract.
        // Note: If the execution state root is not set, then the proof wil not verify.
        StorageSlotProofOutputs memory sspo =
            StorageSlotProofOutputs({storageRoot: executionStateRoot, storageSlots: _storageSlots});

        ISP1Verifier(verifier).verifyProof(storageSlotVkey, abi.encode(sspo), proof);
    }

    function latestExecutionStateRoot() public view returns (bytes32) {
        return executionStateRoots[executionBlockNumber];
    }

    function latestExecutionBlockNumber() public view returns (uint256) {
        return executionBlockNumber;
    }

    /// @notice Gets the sync committee period from a slot.
    function getSyncCommitteePeriod(uint256 slot) public view returns (uint256) {
        return slot / SLOTS_PER_PERIOD;
    }

    /// @notice Gets the current epoch
    function getCurrentEpoch() public view returns (uint256) {
        return head / SLOTS_PER_EPOCH;
    }

    /// @notice Gets the storage slot for a given block number, contract address, and key.
    function getStorageSlot(uint256 blockNumber, address contractAddress, bytes32 key)
        external
        view
        returns (bytes32)
    {
        return storageSlots[computeStorageSlotKey(blockNumber, contractAddress, key)];
    }

    /// @notice Updates the Helios program verification key.
    function updateLightClientVkey(bytes32 newVkey) external onlyGuardian {
        lightClientVkey = newVkey;

        emit LightClientVkeyUpdate(newVkey);
    }

    /// @notice Updates the storage slot proof verification key.
    function updateStorageSlotVkey(bytes32 newVkey) external onlyGuardian {
        storageSlotVkey = newVkey;

        emit StorageSlotVkeyUpdate(newVkey);
    }

    function changeGuardian(address newGuardian) external onlyGuardian {
        require(
            newGuardian != address(0),
            "New guardian cannot be the zero, use relinquishGuardian instead"
        );

        guardian = newGuardian;

        emit GuardianUpdate(newGuardian);
    }

    function relinquishGuardian() external onlyGuardian {
        guardian = address(0);

        emit GuardianRelinquished();
    }

    /// @notice Computes the corresponding key for a storage slot.
    function computeStorageSlotKey(uint256 blockNumber, address contractAddress, bytes32 key)
        internal
        pure
        returns (bytes32)
    {
        return keccak256(abi.encode(blockNumber, contractAddress, key));
    }
}
