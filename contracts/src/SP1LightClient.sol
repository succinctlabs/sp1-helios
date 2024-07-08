// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.16;

import {OutputReader} from "./OutputReader.sol";
import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";

contract SP1LightClient {
    bytes32 public immutable GENESIS_VALIDATORS_ROOT;
    uint256 public immutable GENESIS_TIME;
    uint256 public immutable SECONDS_PER_SLOT;
    uint256 public immutable SLOTS_PER_PERIOD;
    uint32 public immutable SOURCE_CHAIN_ID;
    uint16 public immutable FINALITY_THRESHOLD;
    /// @dev DEPECATED: Do not use. Compatibility for upgrades from TelepathyX.
    bytes32 public immutable STEP_FUNCTION_ID_deprecated;
    /// @dev DEPECATED: Do not use. Compatibility for upgrades from TelepathyX.
    bytes32 public immutable ROTATE_FUNCTION_ID_deprecated;
    /// @dev DEPECATED: Do not use. Compatibility for upgrades from TelepathyX.
    address public immutable FUNCTION_GATEWAY_ADDRESS_deprecated;

    /// @notice The latest slot the light client has a finalized header for.
    uint256 public head = 0;

    /// @notice Maps from a slot to a beacon block header root.
    mapping(uint256 => bytes32) public headers;

    /// @notice Maps from a slot to the timestamp of when the headers mapping was updated with slot as a key
    mapping(uint256 => uint256) public timestamps;

    /// @notice Maps from a slot to the current finalized ethereum1 execution state root.
    mapping(uint256 => bytes32) public executionStateRoots;

    /// @notice Maps from a period to the poseidon commitment for the sync committee.
    mapping(uint256 => bytes32) public syncCommitteePoseidons;

    /// @notice The verification key for the SP1Telepathy program.
    bytes32 public telepathyProgramVkey;

    /// @notice The deployed SP1 verifier contract.
    ISP1Verifier public verifier;

    event HeadUpdate(uint256 indexed slot, bytes32 indexed root);
    event SyncCommitteeUpdate(uint256 indexed period, bytes32 indexed root);

    error SyncCommitteeNotSet(uint256 period);
    error HeaderRootNotSet(uint256 slot);
    error SlotBehindHead(uint64 slot);
    error NotEnoughParticipation(uint16 participation);
    error SyncCommitteeAlreadySet(uint256 period);
    error HeaderRootAlreadySet(uint256 slot);
    error StateRootAlreadySet(uint256 slot);

    constructor(
        bytes32 genesisValidatorsRoot,
        uint256 genesisTime,
        uint256 secondsPerSlot,
        uint256 slotsPerPeriod,
        uint256 syncCommitteePeriod,
        bytes32 syncCommitteePoseidon,
        uint32 sourceChainId,
        uint16 finalityThreshold,
        bytes32 telepathyProgramVkey,
        address verifier
    ) {
        GENESIS_VALIDATORS_ROOT = genesisValidatorsRoot;
        GENESIS_TIME = genesisTime;
        SECONDS_PER_SLOT = secondsPerSlot;
        SLOTS_PER_PERIOD = slotsPerPeriod;
        SOURCE_CHAIN_ID = sourceChainId;
        FINALITY_THRESHOLD = finalityThreshold;
        telepathyProgramVkey = telepathyProgramVkey;
        verifier = ISP1Verifier(verifier);

        setSyncCommitteePoseidon(syncCommitteePeriod, syncCommitteePoseidon);
    }

    /// @notice Process a step proof that has been verified in the FunctionGateway, then move the head forward and store the new roots.
    function step(uint256 attestedSlot) external {
        uint256 period = getSyncCommitteePeriod(attestedSlot);
        bytes32 syncCommitteePoseidon = syncCommitteePoseidons[period];
        if (syncCommitteePoseidon == bytes32(0)) {
            revert SyncCommitteeNotSet(period);
        }

        // Input: [uint256 syncCommitteePoseidon, uint64 attestedSlot]
        // Output: [bytes32 finalizedHeaderRoot, bytes32 executionStateRoot, uint64 finalizedSlot, uint16 participation]
        bytes memory output = IFunctionGateway(FUNCTION_GATEWAY_ADDRESS)
            .verifiedCall(
                STEP_FUNCTION_ID,
                abi.encodePacked(syncCommitteePoseidon, uint64(attestedSlot))
            );
        bytes32 finalizedHeaderRoot = bytes32(
            OutputReader.readUint256(output, 0)
        );
        bytes32 executionStateRoot = bytes32(
            OutputReader.readUint256(output, 32)
        );
        uint64 finalizedSlot = OutputReader.readUint64(output, 64);
        uint16 participation = OutputReader.readUint16(output, 72);

        if (participation < FINALITY_THRESHOLD) {
            revert NotEnoughParticipation(participation);
        }

        if (finalizedSlot <= head) {
            revert SlotBehindHead(finalizedSlot);
        }

        setSlotRoots(
            uint256(finalizedSlot),
            finalizedHeaderRoot,
            executionStateRoot
        );
    }

    /// @notice Process a rotate proof that has been verified in the FunctionGateway, then store the next sync committee poseidon.
    function rotate(uint256 finalizedSlot) external {
        bytes32 finalizedHeaderRoot = headers[finalizedSlot];
        if (finalizedHeaderRoot == bytes32(0)) {
            revert HeaderRootNotSet(finalizedSlot);
        }

        // Input: [bytes32 finalizedHeaderRoot]
        // Output: [bytes32 syncCommitteePoseidon]
        bytes memory output = IFunctionGateway(FUNCTION_GATEWAY_ADDRESS)
            .verifiedCall(
                ROTATE_FUNCTION_ID,
                abi.encodePacked(finalizedHeaderRoot)
            );
        bytes32 syncCommitteePoseidon = bytes32(
            OutputReader.readUint256(output, 0)
        );

        uint256 period = getSyncCommitteePeriod(finalizedSlot);
        uint256 nextPeriod = period + 1;
        setSyncCommitteePoseidon(nextPeriod, syncCommitteePoseidon);
    }

    /// @notice Gets the sync committee period from a slot.
    function getSyncCommitteePeriod(
        uint256 slot
    ) internal view returns (uint256) {
        return slot / SLOTS_PER_PERIOD;
    }

    /// @notice Gets the current slot for the chain the light client is reflecting.
    function getCurrentSlot() internal view returns (uint256) {
        return (block.timestamp - GENESIS_TIME) / SECONDS_PER_SLOT;
    }

    /// @notice Sets the current slot for the chain the light client is reflecting.
    /// @dev Checks if roots exists for the slot already. If there is, check for a conflict between
    ///      the given roots and the existing roots. If there is an existing header but no
    ///      conflict, do nothing. This avoids timestamp renewal DoS attacks.
    function setSlotRoots(
        uint256 slot,
        bytes32 finalizedHeaderRoot,
        bytes32 executionStateRoot
    ) internal {
        if (headers[slot] != bytes32(0)) {
            revert HeaderRootAlreadySet(slot);
        }
        if (executionStateRoots[slot] != bytes32(0)) {
            revert StateRootAlreadySet(slot);
        }
        head = slot;
        headers[slot] = finalizedHeaderRoot;
        executionStateRoots[slot] = executionStateRoot;
        timestamps[slot] = block.timestamp;
        emit HeadUpdate(slot, finalizedHeaderRoot);
    }

    /// @notice Sets the sync committee poseidon for a given period.
    function setSyncCommitteePoseidon(
        uint256 period,
        bytes32 poseidon
    ) internal {
        if (syncCommitteePoseidons[period] != bytes32(0)) {
            revert SyncCommitteeAlreadySet(period);
        }
        syncCommitteePoseidons[period] = poseidon;
        emit SyncCommitteeUpdate(period, poseidon);
    }

    /// @notice Deprecated function, here for compatibility with the old light client.
    function consistent() external pure returns (bool) {
        return true;
    }
}
