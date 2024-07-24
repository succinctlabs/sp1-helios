// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.16;

import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";

/// @title SP1LightClient
/// @notice An ethereum light client, built with SP1.
contract SP1LightClient {
    bytes32 public immutable GENESIS_VALIDATORS_ROOT;
    uint256 public immutable GENESIS_TIME;
    uint256 public immutable SECONDS_PER_SLOT;
    uint256 public immutable SLOTS_PER_PERIOD;
    uint256 public immutable SLOTS_PER_EPOCH;
    uint32 public immutable SOURCE_CHAIN_ID;

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

    /// @notice The verification key for the SP1Telepathy program.
    bytes32 public telepathyProgramVkey;

    /// @notice The deployed SP1 verifier contract.
    ISP1Verifier public verifier;

    /// @notice The address of the guardian
    address public guardian;

    struct ProofOutputs {
        bytes32 prevHeader;
        bytes32 newHeader;
        bytes32 syncCommitteeHash;
        bytes32 nextSyncCommitteeHash;
        uint256 prevHead;
        uint256 newHead;
        bytes32 executionStateRoot;
    }

    event HeadUpdate(uint256 indexed slot, bytes32 indexed root);
    event SyncCommitteeUpdate(uint256 indexed period, bytes32 indexed root);

    error HeaderRootNotConnected(bytes32 header);
    error SlotBehindHead(uint256 slot);
    error SlotNotConnected(uint256 slot);
    error SyncCommitteeAlreadySet(uint256 period);
    error HeaderRootAlreadySet(uint256 slot);
    error StateRootAlreadySet(uint256 slot);

    constructor(
        bytes32 _genesisValidatorsRoot,
        uint256 _genesisTime,
        uint256 _secondsPerSlot,
        uint256 _slotsPerPeriod,
        uint256 _slotsPerEpoch,
        bytes32 _syncCommitteeHash,
        bytes32 _header,
        bytes32 _executionStateRoot,
        uint256 _head,
        bytes32 _telepathyProgramVkey,
        address _verifier,
        address _guardian
    ) {
        GENESIS_VALIDATORS_ROOT = _genesisValidatorsRoot;
        GENESIS_TIME = _genesisTime;
        SECONDS_PER_SLOT = _secondsPerSlot;
        SLOTS_PER_PERIOD = _slotsPerPeriod;
        SLOTS_PER_EPOCH = _slotsPerEpoch;
        syncCommittees[getSyncCommitteePeriod(_head)] = _syncCommitteeHash;
        telepathyProgramVkey = _telepathyProgramVkey;
        headers[_head] = _header;
        executionStateRoots[_head] = _executionStateRoot;
        head = _head;
        verifier = ISP1Verifier(_verifier);
        guardian = _guardian;
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

        if (po.prevHead != head) {
            revert SlotNotConnected(po.prevHead);
        }

        if (po.prevHeader != headers[po.prevHead]) {
            revert HeaderRootNotConnected(po.prevHeader);
        }

        // Verify the proof with the associated public values. This will revert if proof invalid.
        verifier.verifyProof(telepathyProgramVkey, publicValues, proof);

        head = po.newHead;
        if (headers[po.newHead] != bytes32(0)) {
            revert HeaderRootAlreadySet(po.newHead);
        }
        headers[po.newHead] = po.newHeader;
        if (executionStateRoots[po.newHead] != bytes32(0)) {
            revert StateRootAlreadySet(po.newHead);
        }
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
    function getSyncCommitteePeriod(
        uint256 slot
    ) public view returns (uint256) {
        return slot / SLOTS_PER_PERIOD;
    }

    /// @notice Gets the current epoch
    function getCurrentEpoch() public view returns (uint256) {
        return head / SLOTS_PER_EPOCH;
    }

    /// @notice Updates the telepathy program vKey. Call when changing the telepathy program (e.g. adding a new constraint or updating a dependency)
    function updateTelepathyProgramVkey(bytes32 newVkey) external onlyGuardian {
        telepathyProgramVkey = newVkey;
    }
}
