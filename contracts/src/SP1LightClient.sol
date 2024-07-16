// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.16;

import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";

contract SP1LightClient {
    bytes32 public immutable GENESIS_VALIDATORS_ROOT;
    uint256 public immutable GENESIS_TIME;
    uint256 public immutable SECONDS_PER_SLOT;
    uint256 public immutable SLOTS_PER_PERIOD;
    uint256 public immutable SLOTS_PER_EPOCH;
    uint32 public immutable SOURCE_CHAIN_ID;

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

    struct ProofOutputs {
        bytes32 prevHeader;
        bytes32 newHeader;
        bytes32 prevSyncCommitteeHash;
        bytes32 newSyncCommitteeHash;
        uint256 prevHead;
        uint256 newHead;
    }

    event HeadUpdate(uint256 indexed slot, bytes32 indexed root);
    event SyncCommitteeUpdate(uint256 indexed period, bytes32 indexed root);

    error HeaderRootNotConnected(bytes32 header);
    error SlotBehindHead(uint256 slot);
    error SlotNotConnected(uint256 slot);
    error SyncCommitteeNotConnected(bytes32 committe);

    constructor(
        bytes32 _genesisValidatorsRoot,
        uint256 _genesisTime,
        uint256 _secondsPerSlot,
        uint256 _slotsPerPeriod,
        uint256 _slotsPerEpoch,
        bytes32 _syncCommitteeHash,
        bytes32 _finalizedHeader,
        uint256 _head,
        bytes32 _telepathyProgramVkey,
        address _verifier
    ) {
        GENESIS_VALIDATORS_ROOT = _genesisValidatorsRoot;
        GENESIS_TIME = _genesisTime;
        SECONDS_PER_SLOT = _secondsPerSlot;
        SLOTS_PER_PERIOD = _slotsPerPeriod;
        SLOTS_PER_EPOCH = _slotsPerEpoch;
        syncCommittees[getSyncCommitteePeriod(_head)] = _syncCommitteeHash;
        telepathyProgramVkey = _telepathyProgramVkey;
        headers[_head] = _finalizedHeader;
        head = _head;
        verifier = ISP1Verifier(_verifier);
    }

   
    /// @notice Updates the light client with a new header and sync committee (if changed)
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

        if (po.prevSyncCommitteeHash != syncCommittees[getSyncCommitteePeriod(po.prevHead)]) {
            revert SyncCommitteeNotConnected(po.prevSyncCommitteeHash);
        }

        // Verify the proof with the associated public values. This will revert if proof invalid.
        verifier.verifyProof(telepathyProgramVkey, publicValues, proof);

        head = po.newHead;
        headers[po.newHead] = po.newHeader;
        emit HeadUpdate(po.newHead, po.newHeader);

        // Sync commitee isn't always updated for a new head
        if (po.newSyncCommitteeHash != syncCommittees[getSyncCommitteePeriod(po.prevHead)]) {
            syncCommittees[getSyncCommitteePeriod(po.newHead)] = po.newSyncCommitteeHash;
            emit SyncCommitteeUpdate(getSyncCommitteePeriod(po.newHead), po.newSyncCommitteeHash);
        }
    }

    /// @notice Gets the sync committee period from a slot.
    function getSyncCommitteePeriod(
        uint256 slot
    ) internal view returns (uint256) {
        return slot / SLOTS_PER_PERIOD;
    }

    /// @notice Gets the current epoch
    function getCurrentEpoch() public view returns (uint256) {
        return head / SLOTS_PER_EPOCH;
    }
}
