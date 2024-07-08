// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script, console} from "forge-std/Script.sol";
import "forge-std/StdJson.sol";
import {LightClient} from "../src/LightClient.sol";

contract DeployLightClient is Script {
    using stdJson for string;

    function setUp() public {}

    function run() public {
        string memory rawJson;
        {
            string memory CONSENSUS_RPC = vm.envString("CONSENSUS_RPC");
            string[] memory curlInputs = new string[](5);
            curlInputs[0] = "curl";
            curlInputs[1] = string(
                abi.encodePacked(
                    CONSENSUS_RPC,
                    "/api/beacon/proof/lightclient/init/finalized"
                )
            );
            curlInputs[2] = "-o";
            curlInputs[3] = "init.json";
            curlInputs[4] = "--silent";

            vm.ffi(curlInputs);

            rawJson = vm.readFile("init.json");
        }

        bytes32 genesisValidatorsRoot = rawJson.readBytes32(
            ".genesisValidatorsRoot"
        );
        uint256 genesisTime = rawJson.readUint(".genesisTime");
        uint256 secondsPerSlot = rawJson.readUint(".secondsPerSlot");
        uint256 slotsPerPeriod = rawJson.readUint(".slotsPerPeriod");
        uint32 sourceChainId = uint32(rawJson.readUint(".sourceChainId"));
        uint256 syncCommitteePeriod = rawJson.readUint(".syncCommitteePeriod");
        bytes32 syncCommitteePoseidon = rawJson.readBytes32(
            ".syncCommitteePoseidon"
        );

        console.log("genesisValidatorsRoot:");
        console.logBytes32(bytes32(genesisValidatorsRoot));
        console.log("genesisTime: %s", uint256(genesisTime));
        console.log("secondsPerSlot: %s", uint256(secondsPerSlot));
        console.log("slotsPerPeriod: %s", uint256(slotsPerPeriod));
        console.log("syncCommitteePeriod: %s", uint256(syncCommitteePeriod));
        console.log("syncCommitteePoseidon:");
        console.logBytes32(bytes32(syncCommitteePoseidon));
        console.log("sourceChainId: %s", uint32(sourceChainId));

        bytes32 STEP_FUNCTION_ID = vm.envBytes32("STEP_FUNCTION_ID");
        bytes32 ROTATE_FUNCTION_ID = vm.envBytes32("ROTATE_FUNCTION_ID");
        address FUNCTION_GATEWAY_ADDRESS = vm.envAddress("FUNCTION_GATEWAY");
        uint16 FINALITY_THRESHOLD = uint16(vm.envUint("FINALITY_THRESHOLD"));
        console.log("finalityThreshold: %s", uint16(FINALITY_THRESHOLD));
        console.log("stepFunctionId:");
        console.logBytes32(bytes32(STEP_FUNCTION_ID));
        console.log("rotateFunctionId:");
        console.logBytes32(bytes32(ROTATE_FUNCTION_ID));
        console.log(
            "functionGatewayAddress: %s",
            address(FUNCTION_GATEWAY_ADDRESS)
        );
        vm.startBroadcast();
        LightClient lightClient = new LightClient(
            genesisValidatorsRoot,
            genesisTime,
            secondsPerSlot,
            slotsPerPeriod,
            syncCommitteePeriod,
            syncCommitteePoseidon,
            sourceChainId,
            FINALITY_THRESHOLD,
            STEP_FUNCTION_ID,
            ROTATE_FUNCTION_ID,
            FUNCTION_GATEWAY_ADDRESS
        );
        vm.stopBroadcast();
        console.log("LightClient address: %s", address(lightClient));
    }
}
