# R0VM Helios

## Overview

R0VM Helios verifies the consensus of a source chain in the execution environment of a destination chain. For example,
you can run an R0VM Helios light client on Polygon that verifies Ethereum Mainnet's consensus.

R0VM is a fork of [SP1 Helios](https://github.com/succinctlabs/sp1-helios).

## Prerequisites

- [Foundry](https://book.getfoundry.sh/getting-started/installation)
- [RISC Zero](https://dev.risczero.com/api/zkvm/install)

## Steps

### 1. Consensus RPC Setup

To run R0VM Helios, you need a Beacon Chain node for your source chain. For example, to run an Ethereum mainnet light
client, you need an Ethereum mainnet beacon node.

The beacon chain node must support the RPC methods for
the [Altair light client protocol](https://github.com/ethereum/consensus-specs/blob/dev/specs/altair/light-client/sync-protocol.md).
As of 10/15/24, Nimbus is the only consensus client that supports these "light sync" endpoints by default.

There are a few options for setting up a consensus RPC with "light sync" endpoints:

1. Get an RPC from a provider running Nimbus nodes. [Chainstack](https://chainstack.com/) is currently the only provider
   we're aware of that supports this. Set up a node on Chainstack and use the consensus client endpoint for an Ethereum
   mainnet node.
2. Run a Nimbus eth2 beacon node. Instructions [here](https://nimbus.guide/el-light-client.html).
3. There is a community-maintained list of Ethereum Beacon Chain light sync
   endpoints [here](https://s1na.github.io/light-sync-endpoints). These endpoints are not guaranteed to work, and are
   often unreliable.

The RPC you just set up will be used as the `SOURCE_CONSENSUS_RPC_URL` in the next step.

### 2. Environment Setup

In the root directory, create a file called `.env` (mirroring `.env.example`) and set the following environment
variables:

| Parameter                  | Description                                                     |
|----------------------------|-----------------------------------------------------------------|
| `SOURCE_CHAIN_ID`          | Chain ID for the source chain                                   |
| `SOURCE_CONSENSUS_RPC_URL` | RPC URL for the source chain                                    |
| `DEST_RPC_URL`             | RPC URL for the destination chain                               |
| `DEST_CHAIN_ID`            | Chain ID for the destination chain                              |
| `PRIVATE_KEY`              | Private key for the account that will be deploying the contract |

#### Optional Parameters

| Parameter          | Description                                                                            |
|--------------------|----------------------------------------------------------------------------------------|
| `GUARDIAN_ADDRESS` | Defines the owner for the light client. Defaults to the account owner of `PRIVATE_KEY` |
| `LOOP_DELAY_MINS`  | The delay between each loop of the operator in minutes. Defaults to `5`                |

### 3. Deploy Contract

Deploy the R0VM Helios contract:

```bash
cd contracts

# Install dependencies
forge install

# Deploy contract
forge script script/Deploy.s.sol --ffi --rpc-url $DEST_RPC_URL --private-key $PRIVATE_KEY --broadcast
```

When the script completes, take note of the light client contract address printed by the script and add it to your
`.env` file:

| Parameter          | Description                                   |
|--------------------|-----------------------------------------------|
| `CONTRACT_ADDRESS` | Address of the light client contract deployed |

### 4. Run Light Client

To run the operator, which generates proofs and keeps the light client updated with chain state:

```bash
RUST_LOG=info cargo run --release --bin operator
```

If successful, you should see logs indicating that the consensus state is being updated:

```shell
[2025-03-24T18:14:37Z INFO  operator] Starting R0VM Helios operator
[2025-03-24T18:14:38Z WARN  helios::consensus] checkpoint too old, consider using a more recent block
[2025-03-24T18:14:39Z INFO  operator] Contract is up to date. Nothing to update.
[2025-03-24T18:14:39Z INFO  operator] Sleeping for 5 minutes
...
[2025-03-24T18:20:12Z INFO  operator] Attempting to update to new head block: 11334624
[2025-03-24T18:20:12Z INFO  operator] Successfully updated to new head block! Tx hash: 0xae4b00438cfc7be7071c2a6eccf6b3f450086b03210eecf6cd524c17ea404630
[2025-03-24T18:20:12Z INFO  operator] Sleeping for 5 minutes
```
#### Dev Mode

R0VM Helios is compatible with [dev-mode](https://dev.risczero.com/api/generating-proofs/dev-mode).
By setting `RISC0_DEV_MODE=1`, when [deploying the contract](#3-deploy-contract) and running the light client, the actual proving can be skipped for quicker development and testing.
