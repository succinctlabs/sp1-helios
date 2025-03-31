// SPDX-License-Identifier: MIT
pragma solidity ^0.8.22;

import {Test, console2} from "forge-std/Test.sol";
import {SP1Helios} from "../src/SP1Helios.sol";
import {SP1MockVerifier} from "@sp1-contracts/SP1MockVerifier.sol";
import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";
import {IAccessControl} from "@openzeppelin/access/IAccessControl.sol";

contract SP1HeliosTest is Test {
    SP1Helios helios;
    SP1MockVerifier mockVerifier;
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
    bytes32 constant HELIOS_PROGRAM_VKEY = bytes32(uint256(5));
    uint256 constant INITIAL_HEAD = 100;

    function setUp() public {
        mockVerifier = new SP1MockVerifier();

        // Create array of updaters
        address[] memory updatersArray = new address[](1);
        updatersArray[0] = initialUpdater;

        SP1Helios.InitParams memory params = SP1Helios.InitParams({
            executionStateRoot: INITIAL_EXECUTION_STATE_ROOT,
            genesisTime: GENESIS_TIME,
            genesisValidatorsRoot: GENESIS_VALIDATORS_ROOT,
            head: INITIAL_HEAD,
            header: INITIAL_HEADER,
            heliosProgramVkey: HELIOS_PROGRAM_VKEY,
            secondsPerSlot: SECONDS_PER_SLOT,
            slotsPerEpoch: SLOTS_PER_EPOCH,
            slotsPerPeriod: SLOTS_PER_PERIOD,
            sourceChainId: SOURCE_CHAIN_ID,
            syncCommitteeHash: INITIAL_SYNC_COMMITTEE_HASH,
            verifier: address(mockVerifier),
            updaters: updatersArray
        });

        helios = new SP1Helios(params);
    }

    function testInitialization() public view {
        assertEq(helios.GENESIS_VALIDATORS_ROOT(), GENESIS_VALIDATORS_ROOT);
        assertEq(helios.GENESIS_TIME(), GENESIS_TIME);
        assertEq(helios.SECONDS_PER_SLOT(), SECONDS_PER_SLOT);
        assertEq(helios.SLOTS_PER_EPOCH(), SLOTS_PER_EPOCH);
        assertEq(helios.SLOTS_PER_PERIOD(), SLOTS_PER_PERIOD);
        assertEq(helios.SOURCE_CHAIN_ID(), SOURCE_CHAIN_ID);
        assertEq(helios.heliosProgramVkey(), HELIOS_PROGRAM_VKEY);
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
        SP1Helios.StorageSlot[] memory slots = new SP1Helios.StorageSlot[](1);
        slots[0] =
            SP1Helios.StorageSlot({key: slot, value: value, contractAddress: contractAddress});

        // Create proof outputs
        SP1Helios.ProofOutputs memory po = SP1Helios.ProofOutputs({
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
        bytes memory proof = new bytes(0);

        // Set block timestamp to be valid
        vm.warp(helios.slotTimestamp(INITIAL_HEAD) + 1 hours);

        // Update with storage slot
        vm.prank(initialUpdater);
        helios.update(proof, publicValues, INITIAL_HEAD);

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
        SP1MockVerifier newMockVerifier = new SP1MockVerifier();

        // Build new params with multiple updaters
        SP1Helios.InitParams memory params = SP1Helios.InitParams({
            executionStateRoot: INITIAL_EXECUTION_STATE_ROOT,
            genesisTime: GENESIS_TIME,
            genesisValidatorsRoot: GENESIS_VALIDATORS_ROOT,
            head: INITIAL_HEAD,
            header: INITIAL_HEADER,
            heliosProgramVkey: HELIOS_PROGRAM_VKEY,
            secondsPerSlot: SECONDS_PER_SLOT,
            slotsPerEpoch: SLOTS_PER_EPOCH,
            slotsPerPeriod: SLOTS_PER_PERIOD,
            sourceChainId: SOURCE_CHAIN_ID,
            syncCommitteeHash: INITIAL_SYNC_COMMITTEE_HASH,
            verifier: address(newMockVerifier),
            updaters: updatersArray
        });

        // Create new contract instance
        SP1Helios fixedUpdaterHelios = new SP1Helios(params);

        // Verify all updaters have the UPDATER_ROLE
        for (uint256 i = 0; i < updatersArray.length; i++) {
            assertTrue(
                fixedUpdaterHelios.hasRole(fixedUpdaterHelios.UPDATER_ROLE(), updatersArray[i])
            );
        }

        // Verify updaters can update (testing just the first one)
        SP1Helios.StorageSlot[] memory slots = new SP1Helios.StorageSlot[](0); // Empty slots array
        SP1Helios.ProofOutputs memory po = SP1Helios.ProofOutputs({
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
        bytes memory proof = new bytes(0);

        // Set block timestamp to be valid
        vm.warp(fixedUpdaterHelios.slotTimestamp(INITIAL_HEAD) + 1 hours);

        // Update should succeed when called by an updater
        vm.prank(updatersArray[0]);
        fixedUpdaterHelios.update(proof, publicValues, INITIAL_HEAD);

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
        SP1Helios.StorageSlot[] memory slots = new SP1Helios.StorageSlot[](3);

        // Slot 1: ERC20 token balance
        slots[0] = SP1Helios.StorageSlot({
            key: bytes32(uint256(100)),
            value: bytes32(uint256(200)),
            contractAddress: address(0xdef)
        });

        // Slot 2: NFT ownership mapping
        slots[1] = SP1Helios.StorageSlot({
            key: keccak256(abi.encode(address(0xabc), uint256(123))),
            value: bytes32(uint256(1)),
            contractAddress: address(0xbbb)
        });

        // Slot 3: Governance proposal state
        slots[2] = SP1Helios.StorageSlot({
            key: keccak256(abi.encode("proposal", uint256(5))),
            value: bytes32(uint256(2)), // 2 might represent "approved" state
            contractAddress: address(0xccc)
        });

        // Create proof outputs
        SP1Helios.ProofOutputs memory po = SP1Helios.ProofOutputs({
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
        bytes memory proof = new bytes(0); // MockVerifier will accept empty proof

        // Set block timestamp to be valid for the update
        vm.warp(helios.slotTimestamp(INITIAL_HEAD) + 1 hours);

        // Test successful update
        vm.expectEmit(true, true, false, true);
        emit SP1Helios.HeadUpdate(newHead, newHeader);

        // Expect events for all storage slots
        for (uint256 i = 0; i < slots.length; i++) {
            vm.expectEmit(true, true, false, true);
            emit SP1Helios.StorageSlotVerified(
                newHead, slots[i].key, slots[i].value, slots[i].contractAddress
            );
        }

        vm.prank(initialUpdater);
        helios.update(proof, publicValues, INITIAL_HEAD);

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

        SP1Helios.StorageSlot[] memory slots = new SP1Helios.StorageSlot[](0); // No storage slots for this test

        SP1Helios.ProofOutputs memory po = SP1Helios.ProofOutputs({
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
            abi.encodeWithSelector(SP1Helios.PreviousHeadNotSet.selector, nonExistentHead)
        );
        helios.update(proof, publicValues, nonExistentHead);
    }

    function testUpdateWithTooOldFromHead() public {
        // Set block timestamp to be more than MAX_SLOT_AGE after the initial head timestamp
        vm.warp(helios.slotTimestamp(INITIAL_HEAD) + helios.MAX_SLOT_AGE() + 1);

        SP1Helios.StorageSlot[] memory slots = new SP1Helios.StorageSlot[](0); // No storage slots for this test

        SP1Helios.ProofOutputs memory po = SP1Helios.ProofOutputs({
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
        vm.expectRevert(abi.encodeWithSelector(SP1Helios.PreviousHeadTooOld.selector, INITIAL_HEAD));
        helios.update(proof, publicValues, INITIAL_HEAD);
    }

    function testUpdateWithNewHeadBehindFromHead() public {
        uint256 newHead = INITIAL_HEAD - 1; // Less than INITIAL_HEAD

        SP1Helios.StorageSlot[] memory slots = new SP1Helios.StorageSlot[](0); // No storage slots for this test

        SP1Helios.ProofOutputs memory po = SP1Helios.ProofOutputs({
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
        vm.expectRevert(abi.encodeWithSelector(SP1Helios.SlotBehindHead.selector, newHead));
        helios.update(proof, publicValues, INITIAL_HEAD);
    }

    function testUpdateWithIncorrectSyncCommitteeHash() public {
        bytes32 wrongSyncCommitteeHash = bytes32(uint256(999));

        SP1Helios.StorageSlot[] memory slots = new SP1Helios.StorageSlot[](0); // No storage slots for this test

        SP1Helios.ProofOutputs memory po = SP1Helios.ProofOutputs({
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
                SP1Helios.SyncCommitteeStartMismatch.selector,
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
        SP1Helios.StorageSlot[] memory slots = new SP1Helios.StorageSlot[](0); // No storage slots for this test
        SP1Helios.ProofOutputs memory po = SP1Helios.ProofOutputs({
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
        SP1MockVerifier newMockVerifier = new SP1MockVerifier();

        // Build new params with no updaters
        SP1Helios.InitParams memory params = SP1Helios.InitParams({
            executionStateRoot: INITIAL_EXECUTION_STATE_ROOT,
            genesisTime: GENESIS_TIME,
            genesisValidatorsRoot: GENESIS_VALIDATORS_ROOT,
            head: INITIAL_HEAD,
            header: INITIAL_HEADER,
            heliosProgramVkey: HELIOS_PROGRAM_VKEY,
            secondsPerSlot: SECONDS_PER_SLOT,
            slotsPerEpoch: SLOTS_PER_EPOCH,
            slotsPerPeriod: SLOTS_PER_PERIOD,
            sourceChainId: SOURCE_CHAIN_ID,
            syncCommitteeHash: INITIAL_SYNC_COMMITTEE_HASH,
            verifier: address(newMockVerifier),
            updaters: updatersArray
        });

        // Expect revert when no updaters are provided
        vm.expectRevert(abi.encodeWithSelector(SP1Helios.NoUpdatersProvided.selector));
        new SP1Helios(params);
    }

    function testAdminAccess() public {
        // Create array with multiple updaters
        address[] memory updatersArray = new address[](2);
        updatersArray[0] = address(0x100);
        updatersArray[1] = address(0x200);

        // Create new mock verifier for a clean test
        SP1MockVerifier newMockVerifier = new SP1MockVerifier();

        // Build new params
        SP1Helios.InitParams memory params = SP1Helios.InitParams({
            executionStateRoot: INITIAL_EXECUTION_STATE_ROOT,
            genesisTime: GENESIS_TIME,
            genesisValidatorsRoot: GENESIS_VALIDATORS_ROOT,
            head: INITIAL_HEAD,
            header: INITIAL_HEADER,
            heliosProgramVkey: HELIOS_PROGRAM_VKEY,
            secondsPerSlot: SECONDS_PER_SLOT,
            slotsPerEpoch: SLOTS_PER_EPOCH,
            slotsPerPeriod: SLOTS_PER_PERIOD,
            sourceChainId: SOURCE_CHAIN_ID,
            syncCommitteeHash: INITIAL_SYNC_COMMITTEE_HASH,
            verifier: address(newMockVerifier),
            updaters: updatersArray
        });

        // Create new contract instance
        SP1Helios immutableHelios = new SP1Helios(params);

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
        SP1Helios.StorageSlot[] memory emptySlots = new SP1Helios.StorageSlot[](0); // No storage slots for this test

        SP1Helios.ProofOutputs memory po1 = SP1Helios.ProofOutputs({
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
        bytes memory proof = new bytes(0);

        // Set block timestamp to be valid for the update
        vm.warp(helios.slotTimestamp(INITIAL_HEAD) + 1 hours);

        // Expect event emissions for head update and sync committee update
        vm.expectEmit(true, true, false, true);
        emit SP1Helios.HeadUpdate(nextPeriodHead, nextHeader);

        vm.expectEmit(true, true, false, true);
        emit SP1Helios.SyncCommitteeUpdate(nextPeriod, nextSyncCommitteeHash);

        vm.prank(initialUpdater);
        helios.update(proof, publicValues1, INITIAL_HEAD);

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
        SP1Helios.StorageSlot[] memory emptySlots = new SP1Helios.StorageSlot[](0); // No storage slots for this test

        SP1Helios.ProofOutputs memory po2 = SP1Helios.ProofOutputs({
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
        bytes memory proof = new bytes(0);

        // Set block timestamp to be valid for the next update
        vm.warp(helios.slotTimestamp(prevHead) + 1 hours);

        // Expect event emissions for the second update
        vm.expectEmit(true, true, false, true);
        emit SP1Helios.HeadUpdate(newHead, newHeader);

        vm.expectEmit(true, true, false, true);
        emit SP1Helios.SyncCommitteeUpdate(period, newSyncCommitteeHash);

        vm.expectEmit(true, true, false, true);
        emit SP1Helios.SyncCommitteeUpdate(period + 1, nextSyncCommitteeHash);

        vm.prank(initialUpdater);
        helios.update(proof, publicValues2, prevHead);

        // Verify the second update
        assertEq(helios.head(), newHead);
        assertEq(helios.headers(newHead), newHeader);
        assertEq(helios.executionStateRoots(newHead), newExecutionStateRoot);
        assertEq(helios.syncCommittees(period), newSyncCommitteeHash);
        assertEq(helios.syncCommittees(period + 1), nextSyncCommitteeHash);
    }
}
