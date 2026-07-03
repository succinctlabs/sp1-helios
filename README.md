# SP1 Helios

## Overview

SP1 Helios verifies the consensus of a source chain in the execution environment of a destination chain. For example, you can run an SP1 Helios light client on Polygon that verifies Ethereum Mainnet's consensus.

[Docs](https://succinctlabs.github.io/sp1-helios/)

## Operator

The operator keeps an on-chain SP1 Helios light client updated by proving finalized source-chain consensus updates and submitting them to the destination-chain SP1 Helios contract.

Proof requests are fulfilled through the [Succinct Prover Network](https://docs.succinct.xyz/docs/sp1/prover-network/quickstart). See `.env.example` for additional configuration.

The operator supports two commitment modes: (1) default mode and (2) execution-header mode.

### 1. Default mode

This mode commits only finalized light-client state and the execution state root.

```sh
cargo run -p sp1-helios-script --bin operator -- \
  --rpc-url "$DESTINATION_RPC_URL" \
  --contract-address "$SP1_HELIOS" \
  --source-chain-id "$SOURCE_CHAIN_ID" \
  --source-consensus-rpc "$SOURCE_CONSENSUS_RPC_URL" \
  --private-key "$DESTINATION_PRIVATE_KEY"
```

### 2. Execution-header mode

This mode commits everything from the default mode, plus the finalized execution
block hash and finalized execution receipts root. Use this when a consumer needs
receipt or log inclusion.

**NOTE:** This adds about 45k gas per successful operator update, mostly from two extra storage writes.

```sh
cargo run -p sp1-helios-script --bin operator -- \
  --rpc-url "$DESTINATION_RPC_URL" \
  --contract-address "$SP1_HELIOS" \
  --source-chain-id "$SOURCE_CHAIN_ID" \
  --source-consensus-rpc "$SOURCE_CONSENSUS_RPC_URL" \
  --private-key "$DESTINATION_PRIVATE_KEY" \
  --commit-execution-header
```
