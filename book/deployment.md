# Deployment

## Overview

SP1 Helios verifies the consensus of a source chain in the execution environment of a destination chain. For example, you can run an SP1 Helios light client on Polygon that verifies Ethereum Mainnet's consensus.

## Prerequisites

- [Foundry](https://book.getfoundry.sh/getting-started/installation)
- [SP1](https://docs.succinct.xyz/getting-started/install.html)

## Steps

### 1. Consensus RPC Setup

To run SP1 Helios, you need a Beacon Chain node for your source chain. For example, to run an Ethereum mainnet light client, you need an Ethereum mainnet beacon node.

The beacon chain node must support the RPC methods for the [Altair light client protocol](https://github.com/ethereum/consensus-specs/blob/dev/specs/altair/light-client/sync-protocol.md). As of 10/15/24, Nimbus is the only consensus client that supports these "light sync" endpoints by default.

There are a few options for setting up a consensus RPC with "light sync" endpoints:

1. Get an RPC from a provider running Nimbus nodes. [Chainstack](https://chainstack.com/) is currently the only provider we're aware of that supports this. Set up a node on Chainstack and use the consensus client endpoint for an Ethereum mainnet node.
2. Run a Nimbus eth2 beacon node. Instructions [here](https://nimbus.guide/el-light-client.html).
3. There is a community-maintained list of Ethereum Beacon Chain light sync endpoints [here](https://s1na.github.io/light-sync-endpoints). These endpoints are not guaranteed to work, and are often unreliable.

The RPC you just set up will be used as the `SOURCE_CONSENSUS_RPC_URL` in the next step.

### 2. Environment Setup

In the root directory, create a file called `.env` (mirroring `.env.example`) and set the following environment variables:

| Parameter | Description |
|-----------|-------------|
| `SOURCE_CHAIN_ID` | Chain ID for the source chain |
| `SOURCE_CONSENSUS_RPC_URL` | RPC URL for the source chain |
| `DEST_RPC_URL` | RPC URL for the destination chain |
| `DEST_CHAIN_ID` | Chain ID for the destination chain |
| `PRIVATE_KEY` | Private key for the account that will be deploying the contract |
| `SP1_PROVER` | Default: `mock`. `network` will generate real proofs using the Succinct Prover Network |
| `ETHERSCAN_API_KEY` | API key for Etherscan verification |

#### Optional Parameters

| Parameter | Description |
|-----------|-------------|
| `GUARDIAN_ADDRESS` | Defines the owner for the light client. Defaults to the account owner of `PRIVATE_KEY` |
| `NETWORK_PRIVATE_KEY` | Required in `network` mode. The private key of the account that will be requesting proofs from the Succinct Prover Network |
| `SP1_VERIFIER_ADDRESS` | Required in `network` mode. The address of the verifier contract |
| `LOOP_DELAY_MINS` | The delay between each loop of the operator in minutes. Defaults to `5` |

### 3. Deploy Contract

Deploy the SP1 Helios contract:

```bash
# Load environment variables
source .env

cd contracts

# Install dependencies
forge install

# Deploy contract
forge script script/Deploy.s.sol --ffi --rpc-url $DEST_RPC_URL --private-key $PRIVATE_KEY --etherscan-api-key $ETHERSCAN_API_KEY --broadcast --verify
```

When the script completes, take note of the light client contract address printed by the script and add it to your `.env` file:

| Parameter | Description |
|-----------|-------------|
| `CONTRACT_ADDRESS` | Address of the light client contract deployed |

### 4. Run Light Client

To run the operator, which generates proofs and keeps the light client updated with chain state:

```bash
RUST_LOG=info cargo run --release --bin operator
```

If successful, you should see logs indicating that the consensus state is being updated:

```shell
[2024-10-15T21:01:19Z INFO  operator] Starting SP1 Helios operator
[2024-10-15T21:01:20Z WARN  helios::consensus] checkpoint too old, consider using a more recent block
[2024-10-15T21:01:20Z INFO  operator] Contract is up to date. Nothing to update.
[2024-10-15T21:01:20Z INFO  operator] Sleeping for 5 minutes
...
[2024-10-15T21:06:35Z INFO  operator] New head: 6107648
[2024-10-15T21:06:50Z INFO  operator] Transaction hash: 0x4a0dfa2922704295ed59bf16840454858a4d17225cdf613387de7605b5a41520
[2024-10-15T21:06:50Z INFO  operator] Sleeping for 5 minutes
```

> Note: When `SP1_PROVER=mock`, the operator will not generate real proofs.
