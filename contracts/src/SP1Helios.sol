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

    struct ProofOutputs {
        bytes32 executionStateRoot;
        bytes32 newHeader;
        bytes32 nextSyncCommitteeHash;
        uint256 newHead;
        bytes32 prevHeader;
        uint256 prevHead;
        // Hash of the sync committee at the new head.
        bytes32 syncCommitteeHash;
        // Hash of the current sync committee that signed the previous update.
        bytes32 startSyncCommitteeHash;
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

    error SlotBehindHead(uint256 slot);
    error SyncCommitteeAlreadySet(uint256 period);
    error HeaderRootAlreadySet(uint256 slot);
    error StateRootAlreadySet(uint256 slot);
    error SyncCommitteeStartMismatch(bytes32 given, bytes32 expected);

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
        // Parse the outputs from the committed public values associated with the proof.
        ProofOutputs memory po = abi.decode(publicValues, (ProofOutputs));
        if (po.newHead <= head) {
            revert SlotBehindHead(po.newHead);
        }

        uint256 currentPeriod = getSyncCommitteePeriod(head);

        // Note: We should always have a sync committee for the current head.
        // The "start" sync committee hash is the hash of the sync committee that should sign the next update.
        bytes32 currentSyncCommitteeHash = syncCommittees[currentPeriod];
        if (currentSyncCommitteeHash != po.startSyncCommitteeHash) {
            revert SyncCommitteeStartMismatch(po.startSyncCommitteeHash, currentSyncCommitteeHash);
        }

        // Verify the proof with the associated public values. This will revert if proof invalid.
        ISP1Verifier(verifier).verifyProof(heliosProgramVkey, publicValues, proof);

        // Check that the new header hasnt been set already.
        head = po.newHead;
        if (headers[po.newHead] != bytes32(0)) {
            revert HeaderRootAlreadySet(po.newHead);
        }

        // Check that the new state root hasnt been set already.
        headers[po.newHead] = po.newHeader;
        if (executionStateRoots[po.newHead] != bytes32(0)) {
            revert StateRootAlreadySet(po.newHead);
        }

        // Finally set the new state root.
        executionStateRoots[po.newHead] = po.executionStateRoot;
        emit HeadUpdate(po.newHead, po.newHeader);

        uint256 period = getSyncCommitteePeriod(head);

        // If the sync committee for the new peroid is not set, set it.
        // This can happen if the light client was very behind and had a lot of updates
        // Note: Only the latest sync committee is stored, not the intermediate ones from every update.
        // This may leave gaps in the sync committee history
        if (syncCommittees[period] == bytes32(0)) {
            syncCommittees[period] = po.syncCommitteeHash;
            emit SyncCommitteeUpdate(period, po.syncCommitteeHash);
        }
        // Set next peroid's sync committee hash if value exists.
        if (po.nextSyncCommitteeHash != bytes32(0)) {
            uint256 nextPeriod = period + 1;

            // If the next sync committee is already correct, we don't need to update it.
            if (syncCommittees[nextPeriod] != po.nextSyncCommitteeHash) {
                if (syncCommittees[nextPeriod] != bytes32(0)) {
                    revert SyncCommitteeAlreadySet(nextPeriod);
                }

                syncCommittees[nextPeriod] = po.nextSyncCommitteeHash;
                emit SyncCommitteeUpdate(nextPeriod, po.nextSyncCommitteeHash);
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
