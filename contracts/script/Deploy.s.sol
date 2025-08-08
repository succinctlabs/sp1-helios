// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.22;

import "forge-std/Script.sol";
import {SP1Helios, InitParams} from "../src/SP1Helios.sol";
import {SP1MockVerifier} from "@sp1-contracts/SP1MockVerifier.sol";
import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";
import {Vm} from "forge-std/Vm.sol";

/// @title DeployScript
/// @notice Deploy script for the SP1Helios contract.
contract DeployScript is Script {
    function setUp() public {}

    function run() public returns (address) {
        vm.startBroadcast();

        InitParams memory params = readGenesisConfig();

        // If the verifier address is set to 0, set it to the address of the mock verifier.
        if (params.verifier == address(0)) {
            params.verifier = address(new SP1MockVerifier());
        }

        // Deploy the SP1 Helios contract.
        SP1Helios helios = new SP1Helios(params);

        return address(helios);
    }

    function readGenesisConfig() public view returns (InitParams memory) {
        string memory root = vm.projectRoot();
        string memory path = string.concat(root, "/", "genesis.json");
        string memory json = vm.readFile(path);

        InitParams memory params = InitParams({
            executionStateRoot: vm.parseJsonBytes32(json, ".executionStateRoot"),
            executionBlockNumber: vm.parseJsonUint(json, ".executionBlockNumber"),
            genesisTime: vm.parseJsonUint(json, ".genesisTime"),
            genesisValidatorsRoot: vm.parseJsonBytes32(json, ".genesisValidatorsRoot"),
            guardian: vm.parseJsonAddress(json, ".guardian"),
            head: vm.parseJsonUint(json, ".head"),
            header: vm.parseJsonBytes32(json, ".header"),
            lightClientVkey: vm.parseJsonBytes32(json, ".lightClientVkey"),
            storageSlotVkey: vm.parseJsonBytes32(json, ".storageSlotVkey"),
            secondsPerSlot: vm.parseJsonUint(json, ".secondsPerSlot"),
            slotsPerEpoch: vm.parseJsonUint(json, ".slotsPerEpoch"),
            slotsPerPeriod: vm.parseJsonUint(json, ".slotsPerPeriod"),
            sourceChainId: vm.parseJsonUint(json, ".sourceChainId"),
            syncCommitteeHash: vm.parseJsonBytes32(json, ".syncCommitteeHash"),
            verifier: vm.parseJsonAddress(json, ".verifier")
        });

        return params;
    }
}
