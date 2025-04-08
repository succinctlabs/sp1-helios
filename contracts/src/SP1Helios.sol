// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.22;

import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";

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

    /// @notice Maps from a slot to a beacon block header root.
    mapping(uint256 => bytes32) public headers;

    /// @notice Maps from a slot to the current finalized ethereum1 execution state root.
    mapping(uint256 => bytes32) public executionStateRoots;

    /// @notice Maps from a period to the hash for the sync committee.
    mapping(uint256 => bytes32) public syncCommittees;

    /// @notice The verification key for the SP1 Helios program.
    bytes32 public heliosProgramVkey;

    /// @notice The deployed SP1 verifier contract.
    address public verifier;

    /// @notice The address of the guardian
    address public guardian;

    /// @notice Semantic version.
    /// @custom:semver v1.1.0
    string public constant version = "v1.1.0";

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

    struct InitParams {
        bytes32 executionStateRoot;
        uint256 genesisTime;
        bytes32 genesisValidatorsRoot;
        address guardian;
        uint256 head;
        bytes32 header;
        bytes32 heliosProgramVkey;
        uint256 secondsPerSlot;
        uint256 slotsPerEpoch;
        uint256 slotsPerPeriod;
        uint256 sourceChainId;
        bytes32 syncCommitteeHash;
        address verifier;
    }

    event HeadUpdate(uint256 indexed slot, bytes32 indexed root);
    event SyncCommitteeUpdate(uint256 indexed period, bytes32 indexed root);

    error PrevHeadMismatch(uint256 given, uint256 expected);
    error PrevHeaderMismatch(bytes32 given, bytes32 expected);
    error SlotBehindHead(uint256 slot);
    error SyncCommitteeAlreadySet(uint256 period);
    error HeaderRootAlreadySet(uint256 slot);
    error StateRootAlreadySet(uint256 slot);
    error SyncCommitteeStartMismatch(bytes32 given, bytes32 expected);
    error SyncCommitteeNotSet(uint256 period);
    error NextSyncCommitteeMismatch(bytes32 given, bytes32 expected);

    constructor(InitParams memory params) {
        GENESIS_VALIDATORS_ROOT = params.genesisValidatorsRoot;
        GENESIS_TIME = params.genesisTime;
        SECONDS_PER_SLOT = params.secondsPerSlot;
        SLOTS_PER_PERIOD = params.slotsPerPeriod;
        SLOTS_PER_EPOCH = params.slotsPerEpoch;
        SOURCE_CHAIN_ID = params.sourceChainId;
        syncCommittees[getSyncCommitteePeriod(params.head)] = params.syncCommitteeHash;
        heliosProgramVkey = params.heliosProgramVkey;
        headers[params.head] = params.header;
        executionStateRoots[params.head] = params.executionStateRoot;
        head = params.head;
        verifier = params.verifier;
        guardian = params.guardian;
    }

    /// @notice Updates the light client with a new header, execution state root, and sync committee (if changed)
    /// @param proof The proof bytes for the SP1 proof.
    /// @param publicValues The public commitments from the SP1 proof.
    function update(bytes calldata proof, bytes calldata publicValues) external {
        // Verify the proof with the associated public values. This will revert if the proof is invalid.
        ISP1Verifier(verifier).verifyProof(heliosProgramVkey, publicValues, proof);

        // Read the proof outputs from the public values.
        ProofOutputs memory po = abi.decode(publicValues, (ProofOutputs));

        // Assert the prevHead matches the head.
        if (po.prevHead != head) {
            revert PrevHeadMismatch(po.prevHead, head);
        }
        // Assert the prevHeader matches the header for the current head.
        if (headers[po.prevHead] != po.prevHeader) {
            revert PrevHeaderMismatch(headers[po.prevHead], po.prevHeader);
        }

        // The sync committee for the current head should always be set.
        uint256 currentPeriod = getSyncCommitteePeriod(head);
        bytes32 currentSyncCommitteeHash = syncCommittees[currentPeriod];
        if (currentSyncCommitteeHash == bytes32(0)) {
            revert SyncCommitteeNotSet(currentPeriod);
        }
        // The sync committee hash used in the proof should match the current sync committee.
        if (currentSyncCommitteeHash != po.prevSyncCommitteeHash) {
            revert SyncCommitteeStartMismatch(po.prevSyncCommitteeHash, currentSyncCommitteeHash);
        }

        // Confirm that the new slot is greater than the current head.
        if (po.newHead <= head) {
            revert SlotBehindHead(po.newHead);
        }
        // Confirm that the new header has not been set already. This check is redundant, but
        // we include it for clarity.
        if (headers[po.newHead] != bytes32(0)) {
            revert HeaderRootAlreadySet(po.newHead);
        }
        // Confirm that the new state root has not been set already. This check is redundant, but
        // we include it for clarity.
        if (executionStateRoots[po.newHead] != bytes32(0)) {
            revert StateRootAlreadySet(po.newHead);
        }
        head = po.newHead;
        headers[po.newHead] = po.newHeader;
        executionStateRoots[po.newHead] = po.executionStateRoot;
        emit HeadUpdate(po.newHead, po.newHeader);

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
            if (syncCommittees[nextPeriod] == bytes32(0)) {
                // If the next sync committee is not set, set it.
                syncCommittees[nextPeriod] = po.nextSyncCommitteeHash;
                emit SyncCommitteeUpdate(nextPeriod, po.nextSyncCommitteeHash);
            } else if (syncCommittees[nextPeriod] != po.nextSyncCommitteeHash) {
                // If the next sync committee is non-zero, it should match the expected value.
                revert NextSyncCommitteeMismatch(
                    syncCommittees[nextPeriod], po.nextSyncCommitteeHash
                );
            }
        }
    }

    /// @notice Gets the sync committee period from a slot.
    function getSyncCommitteePeriod(uint256 slot) public view returns (uint256) {
        return slot / SLOTS_PER_PERIOD;
    }

    /// @notice Gets the current epoch
    function getCurrentEpoch() public view returns (uint256) {
        return head / SLOTS_PER_EPOCH;
    }

    /// @notice Updates the Helios program verification key.
    function updateHeliosProgramVkey(bytes32 newVkey) external onlyGuardian {
        heliosProgramVkey = newVkey;
    }
}
