// SPDX-License-Identifier: MIT
pragma solidity ^0.8.22;

import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";
import {AccessControlEnumerable} from "@openzeppelin/access/extensions/AccessControlEnumerable.sol";

/// @title SP1Helios
/// @notice An Ethereum beacon chain light client, built with SP1 and Helios.
/// @dev This contract uses SP1 zero-knowledge proofs to verify updates to the Ethereum beacon chain state.
/// The contract stores the latest verified beacon chain header, execution state root, and sync committee information.
/// It also provides functionality to verify and store Ethereum storage slot values.
/// Updater permissions are fixed at contract creation time and cannot be modified afterward.
contract SP1Helios is AccessControlEnumerable {
    /// @notice The genesis validators root for the beacon chain
    bytes32 public immutable GENESIS_VALIDATORS_ROOT;

    /// @notice The timestamp at which the beacon chain genesis block was processed
    uint256 public immutable GENESIS_TIME;

    /// @notice The number of seconds in a slot on the beacon chain
    uint256 public immutable SECONDS_PER_SLOT;

    /// @notice The number of slots in a sync committee period
    uint256 public immutable SLOTS_PER_PERIOD;

    /// @notice The number of slots in an epoch on the beacon chain
    uint256 public immutable SLOTS_PER_EPOCH;

    /// @notice The chain ID of the source blockchain
    uint256 public immutable SOURCE_CHAIN_ID;

    /// @notice Role for updater operations
    bytes32 public constant UPDATER_ROLE = keccak256("UPDATER_ROLE");

    /// @notice Maximum number of time behind current timestamp for a block to be used for proving
    /// @dev This is set to 1 week to prevent timing attacks where malicious validators
    /// could retroactively create forks that diverge from the canonical chain. To minimize this
    /// risk, we limit the maximum age of a block to 1 week.
    uint256 public constant MAX_SLOT_AGE = 1 weeks;

    /// @notice The latest slot the light client has a finalized header for
    uint256 public head;

    /// @notice Maps from a slot to a beacon block header root
    mapping(uint256 => bytes32) public headers;

    /// @notice Maps from a slot to the current finalized execution state root
    mapping(uint256 => bytes32) public executionStateRoots;

    /// @notice Maps from a period to the hash for the sync committee
    mapping(uint256 => bytes32) public syncCommittees;

    /// @notice Maps from (block number, contract, slot) tuple to storage value
    mapping(bytes32 => bytes32) public storageValues;

    /// @notice The verification key for the SP1 Helios program
    bytes32 public immutable heliosProgramVkey;

    /// @notice The deployed SP1 verifier contract
    address public immutable verifier;

    /// @notice Represents a storage slot in an Ethereum smart contract
    struct StorageSlot {
        bytes32 key;
        bytes32 value;
        address contractAddress;
    }

    /// @notice The outputs from a verified SP1 proof
    struct ProofOutputs {
        bytes32 executionStateRoot;
        bytes32 newHeader;
        bytes32 nextSyncCommitteeHash;
        uint256 newHead;
        bytes32 prevHeader;
        uint256 prevHead;
        bytes32 syncCommitteeHash;
        bytes32 startSyncCommitteeHash;
        StorageSlot[] slots;
    }

    /// @notice Parameters for initializing the SP1Helios contract
    struct InitParams {
        bytes32 executionStateRoot;
        uint256 genesisTime;
        bytes32 genesisValidatorsRoot;
        uint256 head;
        bytes32 header;
        bytes32 heliosProgramVkey;
        uint256 secondsPerSlot;
        uint256 slotsPerEpoch;
        uint256 slotsPerPeriod;
        uint256 sourceChainId;
        bytes32 syncCommitteeHash;
        address verifier;
        address[] updaters;
    }

    event HeadUpdate(uint256 indexed slot, bytes32 indexed root);

    event SyncCommitteeUpdate(uint256 indexed period, bytes32 indexed root);

    event StorageSlotVerified(
        uint256 indexed head, bytes32 indexed key, bytes32 value, address contractAddress
    );
    event UpdaterAdded(address indexed updater);

    error SlotBehindHead(uint256 slot);
    error SyncCommitteeAlreadySet(uint256 period);
    error InvalidHeaderRoot(uint256 slot);
    error InvalidStateRoot(uint256 slot);
    error SyncCommitteeStartMismatch(bytes32 given, bytes32 expected);
    error PreviousHeadNotSet(uint256 slot);
    error PreviousHeadTooOld(uint256 slot);
    error NoUpdatersProvided();

    /// @notice Initializes the SP1Helios contract with the provided parameters
    /// @dev Sets up immutable contract state and grants the UPDATER_ROLE to the provided updaters
    /// @param params The initialization parameters
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

        // Make sure at least one updater is provided
        if (params.updaters.length == 0) {
            revert NoUpdatersProvided();
        }

        // Make UPDATER_ROLE not have any admin roles that can manage it
        // This freezes the updater set - no role can add or remove updaters
        _setRoleAdmin(UPDATER_ROLE, bytes32(0));

        // Add all updaters
        for (uint256 i = 0; i < params.updaters.length; i++) {
            address updater = params.updaters[i];
            if (updater != address(0)) {
                _grantRole(UPDATER_ROLE, updater);
                emit UpdaterAdded(updater);
            }
        }
    }

    /// @notice Updates the light client with a new header, execution state root, and sync committee (if changed)
    /// @dev Verifies an SP1 proof and updates the light client state based on the proof outputs
    /// @param proof The proof bytes for the SP1 proof
    /// @param publicValues The public commitments from the SP1 proof
    /// @param fromHead The head slot to prove against
    function update(bytes calldata proof, bytes calldata publicValues, uint256 fromHead)
        external
        onlyRole(UPDATER_ROLE)
    {
        if (headers[fromHead] == bytes32(0)) {
            revert PreviousHeadNotSet(fromHead);
        }

        // Check if the head being proved against is older than allowed.
        if (block.timestamp - slotTimestamp(fromHead) > MAX_SLOT_AGE) {
            revert PreviousHeadTooOld(fromHead);
        }

        // Parse the outputs from the committed public values associated with the proof.
        ProofOutputs memory po = abi.decode(publicValues, (ProofOutputs));
        if (po.newHead <= fromHead) {
            revert SlotBehindHead(po.newHead);
        }

        uint256 currentPeriod = getSyncCommitteePeriod(fromHead);

        // Note: We should always have a sync committee for the current head.
        // The "start" sync committee hash is the hash of the sync committee that should sign the next update.
        bytes32 currentSyncCommitteeHash = syncCommittees[currentPeriod];
        if (currentSyncCommitteeHash != po.startSyncCommitteeHash) {
            revert SyncCommitteeStartMismatch(po.startSyncCommitteeHash, currentSyncCommitteeHash);
        }

        // Verify the proof with the associated public values. This will revert if proof invalid.
        ISP1Verifier(verifier).verifyProof(heliosProgramVkey, publicValues, proof);

        // Check that the new header hasnt been set already.
        if (headers[po.newHead] != bytes32(0) && headers[po.newHead] != po.newHeader) {
            revert InvalidHeaderRoot(po.newHead);
        }
        // Set new header.
        headers[po.newHead] = po.newHeader;
        if (head < po.newHead) {
            head = po.newHead;
        }

        // Check that the new state root hasnt been set already.
        if (
            executionStateRoots[po.newHead] != bytes32(0)
                && executionStateRoots[po.newHead] != po.executionStateRoot
        ) {
            revert InvalidStateRoot(po.newHead);
        }

        // Finally set the new state root.
        executionStateRoots[po.newHead] = po.executionStateRoot;
        emit HeadUpdate(po.newHead, po.newHeader);

        // Store all provided storage slot values
        for (uint256 i = 0; i < po.slots.length; i++) {
            StorageSlot memory slot = po.slots[i];
            bytes32 storageKey = computeStorageKey(po.newHead, slot.contractAddress, slot.key);
            storageValues[storageKey] = slot.value;
            emit StorageSlotVerified(po.newHead, slot.key, slot.value, slot.contractAddress);
        }

        uint256 period = getSyncCommitteePeriod(po.newHead);

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

    /// @notice Gets the sync committee period from a slot
    /// @dev A sync committee period consists of 8192 slots (256 epochs)
    /// @param slot The slot number to get the period for
    /// @return The sync committee period number
    function getSyncCommitteePeriod(uint256 slot) public view returns (uint256) {
        return slot / SLOTS_PER_PERIOD;
    }

    /// @notice Gets the current epoch based on the latest head
    /// @dev An epoch consists of 32 slots
    /// @return The current epoch number
    function getCurrentEpoch() public view returns (uint256) {
        return head / SLOTS_PER_EPOCH;
    }

    /// @notice Gets the timestamp of a slot
    /// @dev Calculated from GENESIS_TIME + (slot * SECONDS_PER_SLOT)
    /// @param slot The slot number to get the timestamp for
    /// @return The Unix timestamp in seconds for the given slot
    function slotTimestamp(uint256 slot) public view returns (uint256) {
        return GENESIS_TIME + slot * SECONDS_PER_SLOT;
    }

    /// @notice Gets the timestamp of the latest head
    /// @dev Convenience function that calls slotTimestamp(head)
    /// @return The Unix timestamp in seconds for the current head slot
    function headTimestamp() public view returns (uint256) {
        return slotTimestamp(head);
    }

    /// @notice Computes the key for a contract's storage slot
    /// @dev Creates a unique key for the storage mapping based on block number, contract address, and slot
    /// @param blockNumber The block number where the storage value was retrieved
    /// @param contractAddress The address of the contract containing the storage slot
    /// @param slot The storage slot key
    /// @return A unique key for looking up the storage value
    function computeStorageKey(uint256 blockNumber, address contractAddress, bytes32 slot)
        public
        pure
        returns (bytes32)
    {
        return keccak256(abi.encodePacked(blockNumber, contractAddress, slot));
    }

    /// @notice Gets the value of a storage slot at a specific block
    /// @dev Looks up storage values that have been verified and stored by this contract
    /// @param blockNumber The block number where the storage value was retrieved
    /// @param contractAddress The address of the contract containing the storage slot
    /// @param slot The storage slot key
    /// @return The value of the storage slot, or zero if not found
    function getStorageSlot(uint256 blockNumber, address contractAddress, bytes32 slot)
        external
        view
        returns (bytes32)
    {
        return storageValues[computeStorageKey(blockNumber, contractAddress, slot)];
    }
}
