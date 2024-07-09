// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.16;

import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";

contract SP1LightClient {
    bytes32 public immutable GENESIS_VALIDATORS_ROOT;
    uint256 public immutable GENESIS_TIME;
    uint256 public immutable SECONDS_PER_SLOT;
    uint256 public immutable SLOTS_PER_PERIOD;
    uint32 public immutable SOURCE_CHAIN_ID;

    /// @notice The latest slot the light client has a finalized header for.
    uint256 public head = 0;

    /// @notice The latest finalized header.
    bytes32 public finalizedHeader;

    /// @notice The hash of the current sync committee.
    bytes32 public syncCommitteeHash;

    /// @notice The verification key for the SP1Telepathy program.
    bytes32 public telepathyProgramVkey;

    /// @notice The deployed SP1 verifier contract.
    ISP1Verifier public verifier;

    struct ProofOutputs {
        bytes32 finalizedHeader;
        bytes32 syncCommitteeHash;
    }

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
        bytes32 syncCommitteeHash,
        uint32 sourceChainId,
        uint16 finalityThreshold,
        bytes32 telepathyProgramVkey,
        address verifier
    ) {
        GENESIS_VALIDATORS_ROOT = genesisValidatorsRoot;
        GENESIS_TIME = genesisTime;
        SECONDS_PER_SLOT = secondsPerSlot;
        SLOTS_PER_PERIOD = slotsPerPeriod;
        syncCommitteeHash = syncCommitteeHash;
        SOURCE_CHAIN_ID = sourceChainId;
        FINALITY_THRESHOLD = finalityThreshold;
        telepathyProgramVkey = telepathyProgramVkey;
        verifier = ISP1Verifier(verifier);
    }

   
    /// @notice Updates the light client with a new header and sync committee (if changed)
    /// @param proof The proof bytes for the SP1 proof.
    /// @param publicValues The public commitments from the SP1 proof.
    function update(bytes calldata proof, bytes calldata publicValues) external {
        // Parse the outputs from the committed public values associated with the proof.
        ProofOutputs memory po = abi.decode(publicValues, (ProofOutputs));

        // Verify the proof with the associated public values. This will revert if proof invalid.
        verifier.verifyProof(telepathyProgramVkey, publicValues, proof);
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
}
