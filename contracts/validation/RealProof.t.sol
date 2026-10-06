// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.22;

import {Vm} from "forge-std/Vm.sol";
import {SP1Verifier} from "@sp1-contracts/v6.1.0/SP1VerifierPlonk.sol";
import {
    SP1Helios,
    InitParams,
    ProofOutputs,
    ExecutionHeaderProofOutputs
} from "../src/SP1Helios.sol";

/// @notice Validate a locally generated proof against the real verifier and Helios state.
contract RealProofTest {
    Vm internal constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));

    function test_RealProofUpdatesStateAndRejectsTampering() public {
        string memory json = vm.readFile(vm.envString("SP1_HELIOS_PROOF_PATH"));
        bytes memory proof = vm.parseJsonBytes(json, ".proof");
        bytes memory values = vm.parseJsonBytes(json, ".publicValues");
        bytes32 vkey = vm.parseJsonBytes32(json, ".vkey");
        bool executionMode =
            keccak256(bytes(vm.parseJsonString(json, ".mode"))) == keccak256("execution-header");
        require(proof.length > 100, "real proof required");
        SP1Verifier verifier = new SP1Verifier();
        verifier.verifyProof(vkey, values, proof);

        ExecutionHeaderProofOutputs memory output;
        if (executionMode) {
            output = abi.decode(values, (ExecutionHeaderProofOutputs));
        } else {
            ProofOutputs memory state = abi.decode(values, (ProofOutputs));
            output.prevHeader = state.prevHeader;
            output.prevHead = state.prevHead;
            output.prevSyncCommitteeHash = state.prevSyncCommitteeHash;
            output.newHead = state.newHead;
            output.newHeader = state.newHeader;
            output.executionStateRoot = state.executionStateRoot;
            output.executionBlockNumber = state.executionBlockNumber;
            output.syncCommitteeHash = state.syncCommitteeHash;
            output.nextSyncCommitteeHash = state.nextSyncCommitteeHash;
            output.storageSlots = state.storageSlots;
        }

        InitParams memory params;
        params.head = output.prevHead;
        params.header = output.prevHeader;
        params.syncCommitteeHash = output.prevSyncCommitteeHash;
        params.lightClientVkey = vkey;
        params.executionHeaderVkey = vkey;
        params.guardian = address(this);
        params.verifier = address(verifier);
        params.slotsPerPeriod = 8192;
        params.slotsPerEpoch = 32;
        params.secondsPerSlot = 12;
        SP1Helios helios = new SP1Helios(params);

        bytes memory badProof = abi.encodePacked(proof);
        badProof[badProof.length - 1] ^= bytes1(uint8(1));
        vm.expectRevert();
        _update(helios, badProof, output, executionMode);
        require(helios.head() == output.prevHead, "invalid proof advanced head");

        bytes32 root = output.executionStateRoot;
        output.executionStateRoot ^= bytes32(uint256(1));
        vm.expectRevert();
        _update(helios, proof, output, executionMode);
        output.executionStateRoot = root;
        require(helios.head() == output.prevHead, "invalid values advanced head");

        helios.updateLightClientVkey(bytes32(uint256(1)));
        helios.updateExecutionHeaderVkey(bytes32(uint256(1)));
        vm.expectRevert();
        _update(helios, proof, output, executionMode);
        require(helios.head() == output.prevHead, "wrong key advanced head");
        helios.updateLightClientVkey(vkey);
        helios.updateExecutionHeaderVkey(vkey);

        _update(helios, proof, output, executionMode);
        require(helios.head() == output.newHead, "head mismatch");
        require(helios.headers(output.newHead) == output.newHeader, "header mismatch");
        require(helios.latestExecutionStateRoot() == root, "state root mismatch");
        require(
            helios.latestExecutionBlockNumber() == output.executionBlockNumber,
            "block number mismatch"
        );
        if (executionMode) {
            require(
                helios.latestExecutionBlockHash() == output.executionBlockHash,
                "block hash mismatch"
            );
            require(
                helios.latestExecutionReceiptsRoot() == output.executionReceiptsRoot,
                "receipts root mismatch"
            );
        }
        for (uint256 i; i < output.storageSlots.length; ++i) {
            require(
                helios.getStorageSlot(
                    output.executionBlockNumber,
                    output.storageSlots[i].contractAddress,
                    output.storageSlots[i].key
                ) == output.storageSlots[i].value,
                "storage mismatch"
            );
        }
        vm.expectRevert();
        _update(helios, proof, output, executionMode);
        require(helios.head() == output.newHead, "replay changed head");
    }

    function _update(
        SP1Helios helios,
        bytes memory proof,
        ExecutionHeaderProofOutputs memory output,
        bool executionMode
    ) internal {
        if (executionMode) {
            helios.updateExecutionHeader{gas: 1_000_000}(proof, output);
        } else {
            helios.update{gas: 1_000_000}(
                proof,
                output.newHead,
                output.newHeader,
                output.executionStateRoot,
                output.executionBlockNumber,
                output.syncCommitteeHash,
                output.nextSyncCommitteeHash,
                output.storageSlots
            );
        }
    }
}
