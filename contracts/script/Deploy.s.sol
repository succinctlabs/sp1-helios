// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.22;

import {Script} from "forge-std/Script.sol";
import {R0VMHelios} from "../src/R0VMHelios.sol";
import {RiscZeroCheats} from "risc0/test/RiscZeroCheats.sol";
import {RiscZeroMockVerifier} from "risc0/test/RiscZeroMockVerifier.sol";
import {IRiscZeroVerifier} from "risc0/IRiscZeroVerifier.sol";
import {Vm} from "forge-std/Vm.sol";

/// @title DeployScript
/// @notice Deploy script for the R0VMHelios contract.
contract DeployScript is Script, RiscZeroCheats {
    function setUp() public {}

    function run() public returns (address) {
        vm.startBroadcast();

        // Update the rollup config to match the current chain. If the starting block number is 0, the latest block number and starting output root will be fetched.
        updateGenesisConfig();

        R0VMHelios.InitParams memory params = readGenesisConfig();

        // If the verifier address is set to 0, set it to the address of the mock verifier.
        if (params.verifier == address(0)) {
            params.verifier = address(deployRiscZeroVerifier());
        }

        // Deploy the R0VM Helios contract.
        R0VMHelios helios = new R0VMHelios(params);

        return address(helios);
    }

    function readGenesisConfig() public returns (R0VMHelios.InitParams memory) {
        string memory root = vm.projectRoot();
        string memory path = string.concat(root, "/", "genesis.json");
        string memory json = vm.readFile(path);
        bytes memory data = vm.parseJson(json);
        return abi.decode(data, (R0VMHelios.InitParams));
    }

    function updateGenesisConfig() public {
        // If ENV_FILE is set, pass it to the genesis binary.
        string memory envFile = vm.envOr("ENV_FILE", string(".env"));

        // Build the genesis binary. Use the quiet flag to suppress build output.
        string[] memory inputs = new string[](6);
        inputs[0] = "cargo";
        inputs[1] = "build";
        inputs[2] = "--bin";
        inputs[3] = "genesis";
        inputs[4] = "--release";
        inputs[5] = "--quiet";
        vm.ffi(inputs);

        // Run the genesis binary which updates the genesis config.
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
