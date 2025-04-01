// SPDX-License-Identifier: MIT
pragma solidity ^0.8.22;

import {Test, console2} from "forge-std/Test.sol";
import {R0VMHelios} from "../src/R0VMHelios.sol";
import {RiscZeroMockVerifier} from "risc0/test/RiscZeroMockVerifier.sol";
import {IRiscZeroVerifier, Receipt as RiscZeroReceipt} from "risc0/IRiscZeroVerifier.sol";
import {IAccessControl} from "openzeppelin/contracts/access/IAccessControl.sol";
import {R0VMHelios} from "../src/R0VMHelios.sol";

contract R0VMHeliosTest is Test {
    R0VMHelios helios;
    RiscZeroMockVerifier mockVerifier;
    address initialUpdater = address(0x2);

    // Constants for test setup
    bytes32 constant GENESIS_VALIDATORS_ROOT = bytes32(uint256(1));
    uint256 constant GENESIS_TIME = 1606824023; // Dec 1, 2020
    uint256 constant SECONDS_PER_SLOT = 12;
    uint256 constant SLOTS_PER_EPOCH = 32;
    uint256 constant SLOTS_PER_PERIOD = 8192; // 256 epochs
    uint256 constant SOURCE_CHAIN_ID = 1; // Ethereum mainnet
    bytes32 constant INITIAL_HEADER = bytes32(uint256(2));
    bytes32 constant INITIAL_EXECUTION_STATE_ROOT = bytes32(uint256(3));
    bytes32 constant INITIAL_SYNC_COMMITTEE_HASH = bytes32(uint256(4));
    bytes32 constant HELIOS_IMAGE_ID = bytes32(uint256(5));
    uint256 constant INITIAL_HEAD = 100;

    function setUp() public {
        mockVerifier = new RiscZeroMockVerifier(bytes4(0));

        // Create array of updaters
        address[] memory updatersArray = new address[](1);
        updatersArray[0] = initialUpdater;

        R0VMHelios.InitParams memory params = R0VMHelios.InitParams({
            executionStateRoot: INITIAL_EXECUTION_STATE_ROOT,
            genesisTime: GENESIS_TIME,
            genesisValidatorsRoot: GENESIS_VALIDATORS_ROOT,
            head: INITIAL_HEAD,
            header: INITIAL_HEADER,
            heliosImageId: HELIOS_IMAGE_ID,
            secondsPerSlot: SECONDS_PER_SLOT,
            slotsPerEpoch: SLOTS_PER_EPOCH,
            slotsPerPeriod: SLOTS_PER_PERIOD,
            sourceChainId: SOURCE_CHAIN_ID,
            syncCommitteeHash: INITIAL_SYNC_COMMITTEE_HASH,
            verifier: address(mockVerifier),
            updaters: updatersArray
        });

        helios = new R0VMHelios(params);
    }

    function testInitialization() public view {
        assertEq(helios.GENESIS_VALIDATORS_ROOT(), GENESIS_VALIDATORS_ROOT);
        assertEq(helios.GENESIS_TIME(), GENESIS_TIME);
        assertEq(helios.SECONDS_PER_SLOT(), SECONDS_PER_SLOT);
        assertEq(helios.SLOTS_PER_EPOCH(), SLOTS_PER_EPOCH);
        assertEq(helios.SLOTS_PER_PERIOD(), SLOTS_PER_PERIOD);
        assertEq(helios.SOURCE_CHAIN_ID(), SOURCE_CHAIN_ID);
        assertEq(helios.heliosImageID(), HELIOS_IMAGE_ID);
        assertEq(helios.head(), INITIAL_HEAD);
        assertEq(helios.headers(INITIAL_HEAD), INITIAL_HEADER);
        assertEq(helios.executionStateRoots(INITIAL_HEAD), INITIAL_EXECUTION_STATE_ROOT);
        assertEq(
            helios.syncCommittees(helios.getSyncCommitteePeriod(INITIAL_HEAD)),
            INITIAL_SYNC_COMMITTEE_HASH
        );
        // Check roles
        // UPDATER_ROLE is its own admin now, not DEFAULT_ADMIN_ROLE
        assertTrue(helios.hasRole(helios.UPDATER_ROLE(), initialUpdater));
        assertEq(helios.verifier(), address(mockVerifier));
    }

    function testGetSyncCommitteePeriod() public view {
        uint256 slot = 16384; // 2 * SLOTS_PER_PERIOD
        assertEq(helios.getSyncCommitteePeriod(slot), 2);

        slot = 8191; // SLOTS_PER_PERIOD - 1
        assertEq(helios.getSyncCommitteePeriod(slot), 0);

        slot = 8192; // SLOTS_PER_PERIOD
        assertEq(helios.getSyncCommitteePeriod(slot), 1);
    }

    function testGetCurrentEpoch() public view {
        // Initial head is 100
        assertEq(helios.getCurrentEpoch(), 3); // 100 / 32 = 3.125, truncated to 3
    }

    function testSlotTimestamp() public view {
        uint256 slot1 = 1000;
        assertEq(helios.slotTimestamp(slot1), GENESIS_TIME + slot1 * SECONDS_PER_SLOT);

        uint256 slot2 = 10000000;
        assertEq(helios.slotTimestamp(slot2), 1726824023);

        assertEq(
            helios.slotTimestamp(slot2) - helios.slotTimestamp(slot1),
            (slot2 - slot1) * SECONDS_PER_SLOT
        );
    }

    function testHeadTimestamp() public view {
        assertEq(helios.headTimestamp(), GENESIS_TIME + INITIAL_HEAD * SECONDS_PER_SLOT);
    }

    function testComputeStorageKey() public view {
        uint256 blockNumber = 123;
        address contractAddress = address(0xabc);
        bytes32 slot = bytes32(uint256(456));

        bytes32 expectedKey = keccak256(abi.encodePacked(blockNumber, contractAddress, slot));
        assertEq(helios.computeStorageKey(blockNumber, contractAddress, slot), expectedKey);
    }

    function testGetStorageSlot() public {
        uint256 blockNumber = 123;
        address contractAddress = address(0xabc);
        bytes32 slot = bytes32(uint256(456));
        bytes32 value = bytes32(uint256(789));

        // Create storage slots to be set
        R0VMHelios.StorageSlot[] memory slots = new R0VMHelios.StorageSlot[](1);
        slots[0] =
            R0VMHelios.StorageSlot({key: slot, value: value, contractAddress: contractAddress});

        // Create proof outputs
        R0VMHelios.ProofOutputs memory po = R0VMHelios.ProofOutputs({
            executionStateRoot: bytes32(uint256(11)),
            newHeader: bytes32(uint256(10)),
            nextSyncCommitteeHash: bytes32(0),
            newHead: blockNumber,
            prevHeader: INITIAL_HEADER,
            prevHead: INITIAL_HEAD,
            syncCommitteeHash: INITIAL_SYNC_COMMITTEE_HASH,
            startSyncCommitteeHash: INITIAL_SYNC_COMMITTEE_HASH,
            slots: slots
        });

        bytes memory publicValues = abi.encode(po);
        RiscZeroReceipt memory receipt =
            RiscZeroMockVerifier(helios.verifier()).mockProve(HELIOS_IMAGE_ID, sha256(publicValues));

        // Set block timestamp to be valid
        vm.warp(helios.slotTimestamp(INITIAL_HEAD) + 1 hours);

        // Update with storage slot
        vm.prank(initialUpdater);
        helios.update(receipt.seal, publicValues, INITIAL_HEAD);

        // Verify storage slot value
        assertEq(helios.getStorageSlot(blockNumber, contractAddress, slot), value);
    }

    function testFixedUpdaters() public {
        // Create array with multiple updaters
        address[] memory updatersArray = new address[](3);
        updatersArray[0] = address(0x100);
        updatersArray[1] = address(0x200);
        updatersArray[2] = address(0x300);

        // Create new mock verifier for a clean test
        RiscZeroMockVerifier newMockVerifier = new RiscZeroMockVerifier(bytes4(0));

        // Build new params with multiple updaters
        R0VMHelios.InitParams memory params = R0VMHelios.InitParams({
            executionStateRoot: INITIAL_EXECUTION_STATE_ROOT,
            genesisTime: GENESIS_TIME,
            genesisValidatorsRoot: GENESIS_VALIDATORS_ROOT,
            head: INITIAL_HEAD,
            header: INITIAL_HEADER,
            heliosImageId: HELIOS_IMAGE_ID,
            secondsPerSlot: SECONDS_PER_SLOT,
            slotsPerEpoch: SLOTS_PER_EPOCH,
            slotsPerPeriod: SLOTS_PER_PERIOD,
            sourceChainId: SOURCE_CHAIN_ID,
            syncCommitteeHash: INITIAL_SYNC_COMMITTEE_HASH,
            verifier: address(newMockVerifier),
            updaters: updatersArray
        });

        // Create new contract instance
        R0VMHelios fixedUpdaterHelios = new R0VMHelios(params);

        // Verify all updaters have the UPDATER_ROLE
        for (uint256 i = 0; i < updatersArray.length; i++) {
            assertTrue(
                fixedUpdaterHelios.hasRole(fixedUpdaterHelios.UPDATER_ROLE(), updatersArray[i])
            );
        }

        // Verify updaters can update (testing just the first one)
        R0VMHelios.StorageSlot[] memory slots = new R0VMHelios.StorageSlot[](0); // Empty slots array
        R0VMHelios.ProofOutputs memory po = R0VMHelios.ProofOutputs({
            executionStateRoot: bytes32(uint256(11)),
            newHeader: bytes32(uint256(10)),
            nextSyncCommitteeHash: bytes32(0),
            newHead: INITIAL_HEAD + 1,
            prevHeader: INITIAL_HEADER,
            prevHead: INITIAL_HEAD,
            syncCommitteeHash: INITIAL_SYNC_COMMITTEE_HASH,
            startSyncCommitteeHash: INITIAL_SYNC_COMMITTEE_HASH,
            slots: slots
        });
        bytes memory publicValues = abi.encode(po);
        RiscZeroReceipt memory receipt =
            newMockVerifier.mockProve(HELIOS_IMAGE_ID, sha256(publicValues));

        // Set block timestamp to be valid
        vm.warp(fixedUpdaterHelios.slotTimestamp(INITIAL_HEAD) + 1 hours);

        // Update should succeed when called by an updater
        vm.prank(updatersArray[0]);
        fixedUpdaterHelios.update(receipt.seal, publicValues, INITIAL_HEAD);

        // Verify update was successful
        assertEq(fixedUpdaterHelios.head(), INITIAL_HEAD + 1);
    }

    function testUpdate() public {
        uint256 newHead = INITIAL_HEAD + 100;
        bytes32 newHeader = bytes32(uint256(10));
        bytes32 newExecutionStateRoot = bytes32(uint256(11));
        bytes32 syncCommitteeHash = INITIAL_SYNC_COMMITTEE_HASH;
        bytes32 nextSyncCommitteeHash = bytes32(uint256(12));

        // Create multiple storage slots to be set
        R0VMHelios.StorageSlot[] memory slots = new R0VMHelios.StorageSlot[](3);

        // Slot 1: ERC20 token balance
        slots[0] = R0VMHelios.StorageSlot({
            key: bytes32(uint256(100)),
            value: bytes32(uint256(200)),
            contractAddress: address(0xdef)
        });

        // Slot 2: NFT ownership mapping
        slots[1] = R0VMHelios.StorageSlot({
            key: keccak256(abi.encode(address(0xabc), uint256(123))),
            value: bytes32(uint256(1)),
            contractAddress: address(0xbbb)
        });

        // Slot 3: Governance proposal state
        slots[2] = R0VMHelios.StorageSlot({
            key: keccak256(abi.encode("proposal", uint256(5))),
            value: bytes32(uint256(2)), // 2 might represent "approved" state
            contractAddress: address(0xccc)
        });

        // Create proof outputs
        R0VMHelios.ProofOutputs memory po = R0VMHelios.ProofOutputs({
            executionStateRoot: newExecutionStateRoot,
            newHeader: newHeader,
            nextSyncCommitteeHash: nextSyncCommitteeHash,
            newHead: newHead,
            prevHeader: INITIAL_HEADER,
            prevHead: INITIAL_HEAD,
            syncCommitteeHash: syncCommitteeHash,
            startSyncCommitteeHash: INITIAL_SYNC_COMMITTEE_HASH,
            slots: slots
        });

        bytes memory publicValues = abi.encode(po);
        RiscZeroReceipt memory receipt =
            RiscZeroMockVerifier(helios.verifier()).mockProve(HELIOS_IMAGE_ID, sha256(publicValues));

        // Set block timestamp to be valid for the update
        vm.warp(helios.slotTimestamp(INITIAL_HEAD) + 1 hours);

        // Test successful update
        vm.expectEmit(true, true, false, true);
        emit R0VMHelios.HeadUpdate(newHead, newHeader);

        // Expect events for all storage slots
        for (uint256 i = 0; i < slots.length; i++) {
            vm.expectEmit(true, true, false, true);
            emit R0VMHelios.StorageSlotVerified(
                newHead, slots[i].key, slots[i].value, slots[i].contractAddress
            );
        }

        vm.prank(initialUpdater);
        helios.update(receipt.seal, publicValues, INITIAL_HEAD);

        // Verify state updates
        assertEq(helios.head(), newHead);
        assertEq(helios.headers(newHead), newHeader);
        assertEq(helios.executionStateRoots(newHead), newExecutionStateRoot);

        // Verify all storage slots were set correctly
        for (uint256 i = 0; i < slots.length; i++) {
            assertEq(
                helios.getStorageSlot(newHead, slots[i].contractAddress, slots[i].key),
                slots[i].value,
                string(abi.encodePacked("Storage slot ", i, " was not set correctly"))
            );
        }

        // Verify sync committee updates
        uint256 period = helios.getSyncCommitteePeriod(newHead);
        uint256 nextPeriod = period + 1;
        assertEq(helios.syncCommittees(nextPeriod), nextSyncCommitteeHash);
    }

    function testUpdateWithNonexistentFromHead() public {
        uint256 nonExistentHead = 999999;

        R0VMHelios.StorageSlot[] memory slots = new R0VMHelios.StorageSlot[](0); // No storage slots for this test

        R0VMHelios.ProofOutputs memory po = R0VMHelios.ProofOutputs({
            executionStateRoot: bytes32(0),
            newHeader: bytes32(0),
            nextSyncCommitteeHash: bytes32(0),
            newHead: nonExistentHead + 1,
            prevHeader: bytes32(0),
            prevHead: nonExistentHead,
            syncCommitteeHash: bytes32(0),
            startSyncCommitteeHash: bytes32(0),
            slots: slots
        });

        bytes memory publicValues = abi.encode(po);
        bytes memory proof = new bytes(0);

        vm.prank(initialUpdater);
        vm.expectRevert(
            abi.encodeWithSelector(R0VMHelios.PreviousHeadNotSet.selector, nonExistentHead)
        );
        helios.update(proof, publicValues, nonExistentHead);
    }

    function testUpdateWithTooOldFromHead() public {
        // Set block timestamp to be more than MAX_SLOT_AGE after the initial head timestamp
        vm.warp(helios.slotTimestamp(INITIAL_HEAD) + helios.MAX_SLOT_AGE() + 1);

        R0VMHelios.StorageSlot[] memory slots = new R0VMHelios.StorageSlot[](0); // No storage slots for this test

        R0VMHelios.ProofOutputs memory po = R0VMHelios.ProofOutputs({
            executionStateRoot: bytes32(0),
            newHeader: bytes32(0),
            nextSyncCommitteeHash: bytes32(0),
            newHead: INITIAL_HEAD + 1,
            prevHeader: INITIAL_HEADER,
            prevHead: INITIAL_HEAD,
            syncCommitteeHash: bytes32(0),
            startSyncCommitteeHash: INITIAL_SYNC_COMMITTEE_HASH,
            slots: slots
        });

        bytes memory publicValues = abi.encode(po);
        bytes memory proof = new bytes(0);

        vm.prank(initialUpdater);
        vm.expectRevert(
            abi.encodeWithSelector(R0VMHelios.PreviousHeadTooOld.selector, INITIAL_HEAD)
        );
        helios.update(proof, publicValues, INITIAL_HEAD);
    }

    function testUpdateWithNewHeadBehindFromHead() public {
        uint256 newHead = INITIAL_HEAD - 1; // Less than INITIAL_HEAD

        R0VMHelios.StorageSlot[] memory slots = new R0VMHelios.StorageSlot[](0); // No storage slots for this test

        R0VMHelios.ProofOutputs memory po = R0VMHelios.ProofOutputs({
            executionStateRoot: bytes32(0),
            newHeader: bytes32(0),
            nextSyncCommitteeHash: bytes32(0),
            newHead: newHead,
            prevHeader: INITIAL_HEADER,
            prevHead: INITIAL_HEAD,
            syncCommitteeHash: bytes32(0),
            startSyncCommitteeHash: INITIAL_SYNC_COMMITTEE_HASH,
            slots: slots
        });

        bytes memory publicValues = abi.encode(po);
        bytes memory proof = new bytes(0);

        // Set block timestamp to be valid for the update
        vm.warp(helios.slotTimestamp(INITIAL_HEAD) + 1 hours);

        vm.prank(initialUpdater);
        vm.expectRevert(abi.encodeWithSelector(R0VMHelios.SlotBehindHead.selector, newHead));
        helios.update(proof, publicValues, INITIAL_HEAD);
    }

    function testUpdateWithIncorrectSyncCommitteeHash() public {
        bytes32 wrongSyncCommitteeHash = bytes32(uint256(999));

        R0VMHelios.StorageSlot[] memory slots = new R0VMHelios.StorageSlot[](0); // No storage slots for this test

        R0VMHelios.ProofOutputs memory po = R0VMHelios.ProofOutputs({
            executionStateRoot: bytes32(0),
            newHeader: bytes32(0),
            nextSyncCommitteeHash: bytes32(0),
            newHead: INITIAL_HEAD + 1,
            prevHeader: INITIAL_HEADER,
            prevHead: INITIAL_HEAD,
            syncCommitteeHash: bytes32(0),
            startSyncCommitteeHash: wrongSyncCommitteeHash, // Wrong hash
            slots: slots
        });

        bytes memory publicValues = abi.encode(po);
        bytes memory proof = new bytes(0);

        // Set block timestamp to be valid for the update
        vm.warp(helios.slotTimestamp(INITIAL_HEAD) + 1 hours);

        vm.prank(initialUpdater);
        vm.expectRevert(
            abi.encodeWithSelector(
                R0VMHelios.SyncCommitteeStartMismatch.selector,
                wrongSyncCommitteeHash,
                INITIAL_SYNC_COMMITTEE_HASH
            )
        );
        helios.update(proof, publicValues, INITIAL_HEAD);
    }

    function testRoleBasedAccessControl() public {
        address nonUpdater = address(0x4);

        // Initial updater has the UPDATER_ROLE
        assertTrue(helios.hasRole(helios.UPDATER_ROLE(), initialUpdater));

        // Non-updater cannot call update
        vm.prank(nonUpdater);
        R0VMHelios.StorageSlot[] memory slots = new R0VMHelios.StorageSlot[](0); // No storage slots for this test
        R0VMHelios.ProofOutputs memory po = R0VMHelios.ProofOutputs({
            executionStateRoot: bytes32(uint256(11)),
            newHeader: bytes32(uint256(10)),
            nextSyncCommitteeHash: bytes32(uint256(12)),
            newHead: INITIAL_HEAD + 1,
            prevHeader: INITIAL_HEADER,
            prevHead: INITIAL_HEAD,
            syncCommitteeHash: INITIAL_SYNC_COMMITTEE_HASH,
            startSyncCommitteeHash: INITIAL_SYNC_COMMITTEE_HASH,
            slots: slots
        });
        bytes memory publicValues = abi.encode(po);
        bytes memory proof = new bytes(0);

        vm.expectRevert();
        helios.update(proof, publicValues, INITIAL_HEAD);
    }

    function testNoUpdaters() public {
        // Create empty array for updaters
        address[] memory updatersArray = new address[](0);

        // Create new mock verifier for a clean test
        RiscZeroMockVerifier newMockVerifier = new RiscZeroMockVerifier(bytes4(0));

        // Build new params with no updaters
        R0VMHelios.InitParams memory params = R0VMHelios.InitParams({
            executionStateRoot: INITIAL_EXECUTION_STATE_ROOT,
            genesisTime: GENESIS_TIME,
            genesisValidatorsRoot: GENESIS_VALIDATORS_ROOT,
            head: INITIAL_HEAD,
            header: INITIAL_HEADER,
            heliosImageId: HELIOS_IMAGE_ID,
            secondsPerSlot: SECONDS_PER_SLOT,
            slotsPerEpoch: SLOTS_PER_EPOCH,
            slotsPerPeriod: SLOTS_PER_PERIOD,
            sourceChainId: SOURCE_CHAIN_ID,
            syncCommitteeHash: INITIAL_SYNC_COMMITTEE_HASH,
            verifier: address(newMockVerifier),
            updaters: updatersArray
        });

        // Expect revert when no updaters are provided
        vm.expectRevert(abi.encodeWithSelector(R0VMHelios.NoUpdatersProvided.selector));
        new R0VMHelios(params);
    }

    function testAdminAccess() public {
        // Create array with multiple updaters
        address[] memory updatersArray = new address[](2);
        updatersArray[0] = address(0x100);
        updatersArray[1] = address(0x200);

        // Create new mock verifier for a clean test
        RiscZeroMockVerifier newMockVerifier = new RiscZeroMockVerifier(bytes4(0));

        // Build new params
        R0VMHelios.InitParams memory params = R0VMHelios.InitParams({
            executionStateRoot: INITIAL_EXECUTION_STATE_ROOT,
            genesisTime: GENESIS_TIME,
            genesisValidatorsRoot: GENESIS_VALIDATORS_ROOT,
            head: INITIAL_HEAD,
            header: INITIAL_HEADER,
            heliosImageId: HELIOS_IMAGE_ID,
            secondsPerSlot: SECONDS_PER_SLOT,
            slotsPerEpoch: SLOTS_PER_EPOCH,
            slotsPerPeriod: SLOTS_PER_PERIOD,
            sourceChainId: SOURCE_CHAIN_ID,
            syncCommitteeHash: INITIAL_SYNC_COMMITTEE_HASH,
            verifier: address(newMockVerifier),
            updaters: updatersArray
        });

        // Create new contract instance
        R0VMHelios immutableHelios = new R0VMHelios(params);

        // Verify there's no admin for the UPDATER_ROLE
        bytes32 adminRole = immutableHelios.getRoleAdmin(immutableHelios.UPDATER_ROLE());
        assertEq(adminRole, bytes32(0)); // No admin role

        // Verify even the updater can't add new updaters
        vm.prank(updatersArray[0]);
        // No method to call - these functions have been removed
        // The test just verifies that the role is correctly fixed at initialization
    }

    function testUpdateThroughMultipleSyncCommittees() public {
        // We'll move forward by more than one sync committee period
        uint256 initialPeriod = helios.getSyncCommitteePeriod(INITIAL_HEAD);
        uint256 nextPeriod = initialPeriod + 1;
        uint256 futurePeriod = initialPeriod + 2;

        // First update values
        uint256 nextPeriodHead = INITIAL_HEAD + SLOTS_PER_PERIOD / 2; // Middle of next period
        bytes32 nextHeader = bytes32(uint256(10));
        bytes32 nextExecutionStateRoot = bytes32(uint256(11));
        bytes32 nextSyncCommitteeHash = bytes32(uint256(12));

        // Perform first update (to next period)
        performFirstUpdate(
            nextPeriodHead, nextHeader, nextExecutionStateRoot, nextSyncCommitteeHash, nextPeriod
        );

        // Future update values
        uint256 futurePeriodHead = INITIAL_HEAD + (SLOTS_PER_PERIOD * 2) - 10; // Close to end of second period
        bytes32 futureHeader = bytes32(uint256(20));
        bytes32 futureExecutionStateRoot = bytes32(uint256(21));
        bytes32 futureSyncCommitteeHash = bytes32(uint256(22));
        bytes32 futureNextSyncCommitteeHash = bytes32(uint256(13));

        // Perform second update (to future period)
        performSecondUpdate(
            nextPeriodHead,
            nextHeader,
            bytes32(0), // This parameter is not used
            futurePeriodHead,
            futureHeader,
            futureExecutionStateRoot,
            futureSyncCommitteeHash,
            futureNextSyncCommitteeHash,
            futurePeriod
        );

        // Make sure we've gone through multiple periods
        assertNotEq(initialPeriod, helios.getSyncCommitteePeriod(futurePeriodHead));
        assertEq(futurePeriod, helios.getSyncCommitteePeriod(futurePeriodHead));
    }

    // Helper function for the first update in testUpdateThroughMultipleSyncCommittees
    function performFirstUpdate(
        uint256 nextPeriodHead,
        bytes32 nextHeader,
        bytes32 nextExecutionStateRoot,
        bytes32 nextSyncCommitteeHash,
        uint256 nextPeriod
    ) internal {
        R0VMHelios.StorageSlot[] memory emptySlots = new R0VMHelios.StorageSlot[](0); // No storage slots for this test

        R0VMHelios.ProofOutputs memory po1 = R0VMHelios.ProofOutputs({
            executionStateRoot: nextExecutionStateRoot,
            newHeader: nextHeader,
            nextSyncCommitteeHash: nextSyncCommitteeHash, // For the next period
            newHead: nextPeriodHead,
            prevHeader: INITIAL_HEADER,
            prevHead: INITIAL_HEAD,
            syncCommitteeHash: INITIAL_SYNC_COMMITTEE_HASH,
            startSyncCommitteeHash: INITIAL_SYNC_COMMITTEE_HASH,
            slots: emptySlots
        });

        bytes memory publicValues1 = abi.encode(po1);
        RiscZeroReceipt memory receipt = RiscZeroMockVerifier(helios.verifier()).mockProve(
            HELIOS_IMAGE_ID, sha256(publicValues1)
        );

        // Set block timestamp to be valid for the update
        vm.warp(helios.slotTimestamp(INITIAL_HEAD) + 1 hours);

        // Expect event emissions for head update and sync committee update
        vm.expectEmit(true, true, false, true);
        emit R0VMHelios.HeadUpdate(nextPeriodHead, nextHeader);

        vm.expectEmit(true, true, false, true);
        emit R0VMHelios.SyncCommitteeUpdate(nextPeriod, nextSyncCommitteeHash);

        vm.prank(initialUpdater);
        helios.update(receipt.seal, publicValues1, INITIAL_HEAD);

        // Verify the updates
        assertEq(helios.head(), nextPeriodHead);
        assertEq(helios.headers(nextPeriodHead), nextHeader);
        assertEq(helios.executionStateRoots(nextPeriodHead), nextExecutionStateRoot);
        assertEq(helios.syncCommittees(nextPeriod), nextSyncCommitteeHash);
    }

    // Helper function for the second update in testUpdateThroughMultipleSyncCommittees
    function performSecondUpdate(
        uint256 prevHead,
        bytes32 prevHeader,
        bytes32, /* prevSyncCommitteeHash */
        uint256 newHead,
        bytes32 newHeader,
        bytes32 newExecutionStateRoot,
        bytes32 newSyncCommitteeHash,
        bytes32 nextSyncCommitteeHash,
        uint256 period
    ) internal {
        R0VMHelios.StorageSlot[] memory emptySlots = new R0VMHelios.StorageSlot[](0); // No storage slots for this test

        R0VMHelios.ProofOutputs memory po2 = R0VMHelios.ProofOutputs({
            executionStateRoot: newExecutionStateRoot,
            newHeader: newHeader,
            nextSyncCommitteeHash: nextSyncCommitteeHash, // For the period after futurePeriod
            newHead: newHead,
            prevHeader: prevHeader,
            prevHead: prevHead,
            syncCommitteeHash: newSyncCommitteeHash,
            startSyncCommitteeHash: INITIAL_SYNC_COMMITTEE_HASH, // This must match the sync committee from the initial setup
            slots: emptySlots
        });

        bytes memory publicValues2 = abi.encode(po2);
        RiscZeroReceipt memory receipt = RiscZeroMockVerifier(helios.verifier()).mockProve(
            HELIOS_IMAGE_ID, sha256(publicValues2)
        );

        // Set block timestamp to be valid for the next update
        vm.warp(helios.slotTimestamp(prevHead) + 1 hours);

        // Expect event emissions for the second update
        vm.expectEmit(true, true, false, true);
        emit R0VMHelios.HeadUpdate(newHead, newHeader);

        vm.expectEmit(true, true, false, true);
        emit R0VMHelios.SyncCommitteeUpdate(period, newSyncCommitteeHash);

        vm.expectEmit(true, true, false, true);
        emit R0VMHelios.SyncCommitteeUpdate(period + 1, nextSyncCommitteeHash);

        vm.prank(initialUpdater);
        helios.update(receipt.seal, publicValues2, prevHead);

        // Verify the second update
        assertEq(helios.head(), newHead);
        assertEq(helios.headers(newHead), newHeader);
        assertEq(helios.executionStateRoots(newHead), newExecutionStateRoot);
        assertEq(helios.syncCommittees(period), newSyncCommitteeHash);
        assertEq(helios.syncCommittees(period + 1), nextSyncCommitteeHash);
    }
}
