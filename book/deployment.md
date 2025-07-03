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

### 2. Deploy Contract

Deploy the SP1 Helios contract, note, this requires [Foundry](https://getfoundry.sh/), and a [PLONK verifier gateway](https://docs.succinct.xyz/docs/sp1/verification/contract-addresses):

```bash
cargo run --bin genesis -- [--private-key] [--ledger] [--etherscan-api-key] <--sp1-verifier-address> <--guardian-address> <--source-consensus-rpc> <--source-chain-id> 
```

When the script completes, take note of the light client contract address printed to the terminal.

### 3. Run Light Client

To run the operator, which generates proofs and keeps the light client updated with chain state:

```bash
cargo run --release --bin operator -- <--rpc-url> <--contract-address> <--source-chain-id> <--source-consensus-rpc> <--private-key>
```

Internally the Operator program uses the [SP1EnvProver](https://docs.rs/sp1-sdk/latest/sp1_sdk/env/struct.EnvProver.html#method.new), the docs will explain how to setup the ENV vars.


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
