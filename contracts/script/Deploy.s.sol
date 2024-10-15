// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.22;

import "forge-std/Script.sol";
import {SP1LightClient} from "../src/SP1LightClient.sol";
import {SP1MockVerifier} from "@sp1-contracts/SP1MockVerifier.sol";
import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";
import {Vm} from "forge-std/Vm.sol";

/// @title DeployScript
/// @notice Deploy script for the SP1LightClient contract.
contract DeployScript is Script {
    function setUp() public {}

    function run() public returns (address) {
        vm.startBroadcast();

        ISP1Verifier verifier;
        // Detect if the SP1_PROVER is set to mock, and pick the correct verifier.
        string memory mockStr = "mock";
        if (
            keccak256(abi.encodePacked(vm.envString("SP1_PROVER"))) ==
            keccak256(abi.encodePacked(mockStr))
        ) {
            verifier = ISP1Verifier(address(new SP1MockVerifier()));
        } else {
            verifier = ISP1Verifier(
                address(vm.envAddress("SP1_VERIFIER_ADDRESS"))
            );
        }

        // Read trusted initialization parameters from environment.
        address guardian = vm.envOr("GUARDIAN_ADDRESS", msg.sender);

        SP1LightClient.InitParams memory params = SP1LightClient.InitParams({
            genesisValidatorsRoot: vm.envBytes32("GENESIS_VALIDATORS_ROOT"),
            genesisTime: vm.envUint("GENESIS_TIME"),
            secondsPerSlot: vm.envUint("SECONDS_PER_SLOT"),
            slotsPerPeriod: vm.envUint("SLOTS_PER_PERIOD"),
            slotsPerEpoch: vm.envUint("SLOTS_PER_EPOCH"),
            sourceChainId: vm.envUint("SOURCE_CHAIN_ID"),
            syncCommitteeHash: vm.envBytes32("SYNC_COMMITTEE_HASH"),
            header: vm.envBytes32("FINALIZED_HEADER"),
            executionStateRoot: vm.envBytes32("EXECUTION_STATE_ROOT"),
            head: vm.envUint("HEAD"),
            heliosProgramVkey: vm.envBytes32("SP1_HELIOS_PROGRAM_VKEY"),
            verifier: address(verifier),
            guardian: guardian
        });

        // Deploy the SP1 Helios contract.
        SP1LightClient lightClient = new SP1LightClient{
            salt: bytes32(vm.envBytes("CREATE2_SALT"))
        }(params);

        return address(lightClient);
    }
}
