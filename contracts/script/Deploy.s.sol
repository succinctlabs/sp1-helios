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

        // Update the rollup config to match the current chain. If the starting block number is 0, the latest block number and starting output root will be fetched.
        updateGenesisConfig();

        SP1LightClient.InitParams memory params = readGenesisConfig();

        // Detect if the SP1_VERIFIER_ADDRESS is set to mock, and pick the correct verifier.
        string memory emptyStr = "";
        if (
            keccak256(abi.encodePacked(params.verifier)) ==
            keccak256(abi.encodePacked(emptyStr))
        ) {
            params.verifier = address(new SP1MockVerifier());
        }

        // Deploy the SP1 Helios contract.
        SP1LightClient lightClient = new SP1LightClient(params);

        return address(lightClient);
    }

    function readGenesisConfig()
        public
        returns (SP1LightClient.InitParams memory)
    {
        string memory root = vm.projectRoot();
        string memory path = string.concat(root, "/", "genesis.json");
        string memory json = vm.readFile(path);
        bytes memory data = vm.parseJson(json);
        return abi.decode(data, (SP1LightClient.InitParams));
    }

    function updateGenesisConfig() public {
        // If ENV_FILE is set, pass it to the fetch-rollup-config binary.
        string memory envFile = vm.envOr("ENV_FILE", string(".env"));

        // Build the fetch-rollup-config binary. Use the quiet flag to suppress build output.
        string[] memory inputs = new string[](6);
        inputs[0] = "cargo";
        inputs[1] = "build";
        inputs[2] = "--bin";
        inputs[3] = "genesis";
        inputs[4] = "--release";
        inputs[5] = "--quiet";
        vm.ffi(inputs);

        // Run the fetch-rollup-config binary which updates the rollup config hash and the block number in the config.
        // Use the quiet flag to suppress build output.
        string[] memory inputs2 = new string[](9);
        inputs2[0] = "cargo";
        inputs2[1] = "run";
        inputs2[2] = "--bin";
        inputs2[3] = "genesis";
        inputs2[4] = "--release";
        inputs2[5] = "--quiet";
        inputs2[6] = "--";
        inputs2[7] = "--env-file";
        inputs2[8] = envFile;

        vm.ffi(inputs2);
    }
}
