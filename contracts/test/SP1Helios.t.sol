// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.22;

import {
    SP1Helios,
    InitParams,
    ExecutionHeaderProofOutputs,
    StorageSlot
} from "../src/SP1Helios.sol";

contract MockVerifier {
    bytes32 public expectedVkey;
    bytes32 public expectedProofHash;
    bool public shouldRevert;

    function expectProof(bytes32 vkey, bytes calldata proof) external {
        expectedVkey = vkey;
        expectedProofHash = keccak256(proof);
    }

    function setShouldRevert(bool value) external {
        shouldRevert = value;
    }

    function verifyProof(bytes32 vkey, bytes calldata, bytes calldata proof) external view {
        require(!shouldRevert, "invalid proof");
        require(vkey == expectedVkey, "unexpected vkey");
        require(keccak256(proof) == expectedProofHash, "unexpected proof");
    }
}

contract NonGuardianCaller {
    function updateExecutionHeaderVkey(SP1Helios helios, bytes32 newVkey) external {
        helios.updateExecutionHeaderVkey(newVkey);
    }
}

contract SP1HeliosTest {
    bytes32 internal constant GENESIS_VALIDATORS_ROOT = bytes32(uint256(0x1));
    bytes32 internal constant GENESIS_HEADER = bytes32(uint256(0x2));
    bytes32 internal constant GENESIS_STATE_ROOT = bytes32(uint256(0x3));
    bytes32 internal constant GENESIS_BLOCK_HASH = bytes32(uint256(0x4));
    bytes32 internal constant GENESIS_RECEIPTS_ROOT = bytes32(uint256(0x5));
    bytes32 internal constant SYNC_COMMITTEE = bytes32(uint256(0x6));
    bytes32 internal constant LIGHT_CLIENT_VKEY = bytes32(uint256(0x7));
    bytes32 internal constant EXECUTION_HEADER_VKEY = bytes32(uint256(0x8));
    bytes32 internal constant STORAGE_SLOT_VKEY = bytes32(uint256(0x9));
    bytes32 internal constant NEW_HEADER = bytes32(uint256(0x10));
    bytes32 internal constant NEW_STATE_ROOT = bytes32(uint256(0x11));
    bytes32 internal constant NEW_BLOCK_HASH = bytes32(uint256(0x12));
    bytes32 internal constant NEW_RECEIPTS_ROOT = bytes32(uint256(0x13));
    bytes internal constant PROOF = hex"1234";

    MockVerifier internal verifier;
    SP1Helios internal helios;

    function setUp() public {
        verifier = new MockVerifier();
        helios = new SP1Helios(_initParams());
    }

    function test_ConstructorSeedsExecutionHeaderRoots() public view {
        _assertEq(helios.executionStateRoots(100), GENESIS_STATE_ROOT, "genesis state root");
        _assertEq(helios.executionBlockHashes(100), GENESIS_BLOCK_HASH, "genesis block hash");
        _assertEq(
            helios.executionReceiptsRoots(100), GENESIS_RECEIPTS_ROOT, "genesis receipts root"
        );
        _assertEq(helios.latestExecutionBlockHash(), GENESIS_BLOCK_HASH, "latest block hash");
        _assertEq(
            helios.latestExecutionReceiptsRoot(), GENESIS_RECEIPTS_ROOT, "latest receipts root"
        );
    }

    function test_LegacyUpdatePreservesExistingPath() public {
        verifier.expectProof(LIGHT_CLIENT_VKEY, PROOF);
        helios.update(
            PROOF, 64, NEW_HEADER, NEW_STATE_ROOT, 101, SYNC_COMMITTEE, bytes32(0), _emptySlots()
        );

        _assertEq(helios.latestExecutionStateRoot(), NEW_STATE_ROOT, "latest state root");
        _assertEq(helios.executionBlockHashes(101), bytes32(0), "legacy block hash unset");
        _assertEq(helios.executionReceiptsRoots(101), bytes32(0), "legacy receipts root unset");
    }

    function test_UpdateExecutionHeaderStoresExecutionCommitments() public {
        verifier.expectProof(EXECUTION_HEADER_VKEY, PROOF);
        helios.updateExecutionHeader(PROOF, _executionHeaderProofOutputs());

        _assertEq(helios.latestExecutionStateRoot(), NEW_STATE_ROOT, "latest state root");
        _assertEq(helios.latestExecutionBlockHash(), NEW_BLOCK_HASH, "latest block hash");
        _assertEq(helios.latestExecutionReceiptsRoot(), NEW_RECEIPTS_ROOT, "latest receipts root");
        _assertEq(helios.executionBlockHashes(101), NEW_BLOCK_HASH, "stored block hash");
        _assertEq(helios.executionReceiptsRoots(101), NEW_RECEIPTS_ROOT, "stored receipts root");
    }

    function test_UpdateExecutionHeaderVkeyIsGuardianOnly() public {
        bytes32 newVkey = bytes32(uint256(0xabc));
        helios.updateExecutionHeaderVkey(newVkey);
        _assertEq(helios.executionHeaderVkey(), newVkey, "guardian update");

        NonGuardianCaller caller = new NonGuardianCaller();
        try caller.updateExecutionHeaderVkey(helios, bytes32(uint256(0xdef))) {
            revert("non-guardian update succeeded");
        } catch {}
    }

    function test_RevertWhen_ExecutionHeaderProofFails() public {
        verifier.setShouldRevert(true);

        try helios.updateExecutionHeader(PROOF, _executionHeaderProofOutputs()) {
            revert("invalid proof accepted");
        } catch {}

        _assertEq(helios.latestExecutionBlockNumber(), 100, "block number unchanged");
        _assertEq(helios.latestExecutionBlockHash(), GENESIS_BLOCK_HASH, "block hash unchanged");
    }

    function _initParams() internal view returns (InitParams memory) {
        return InitParams({
            executionStateRoot: GENESIS_STATE_ROOT,
            executionBlockNumber: 100,
            executionBlockHash: GENESIS_BLOCK_HASH,
            executionReceiptsRoot: GENESIS_RECEIPTS_ROOT,
            genesisTime: 1,
            genesisValidatorsRoot: GENESIS_VALIDATORS_ROOT,
            guardian: address(this),
            head: 32,
            header: GENESIS_HEADER,
            lightClientVkey: LIGHT_CLIENT_VKEY,
            executionHeaderVkey: EXECUTION_HEADER_VKEY,
            storageSlotVkey: STORAGE_SLOT_VKEY,
            secondsPerSlot: 12,
            slotsPerEpoch: 32,
            slotsPerPeriod: 8192,
            sourceChainId: 1,
            syncCommitteeHash: SYNC_COMMITTEE,
            verifier: address(verifier)
        });
    }

    function _executionHeaderProofOutputs()
        internal
        pure
        returns (ExecutionHeaderProofOutputs memory)
    {
        return ExecutionHeaderProofOutputs({
            prevHeader: GENESIS_HEADER,
            prevHead: 32,
            prevSyncCommitteeHash: SYNC_COMMITTEE,
            newHead: 64,
            newHeader: NEW_HEADER,
            executionStateRoot: NEW_STATE_ROOT,
            executionBlockNumber: 101,
            executionBlockHash: NEW_BLOCK_HASH,
            executionReceiptsRoot: NEW_RECEIPTS_ROOT,
            syncCommitteeHash: SYNC_COMMITTEE,
            nextSyncCommitteeHash: bytes32(0),
            storageSlots: _emptySlots()
        });
    }

    function _emptySlots() internal pure returns (StorageSlot[] memory) {
        return new StorageSlot[](0);
    }

    function _assertEq(bytes32 actual, bytes32 expected, string memory reason) internal pure {
        require(actual == expected, reason);
    }

    function _assertEq(uint256 actual, uint256 expected, string memory reason) internal pure {
        require(actual == expected, reason);
    }
}
