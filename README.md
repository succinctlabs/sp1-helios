# SP1 Helios

## Overview

SP1 Helios verifies the consensus of a source chain in the execution environment of a destination chain. For example, you can run an SP1 Helios light client on Polygon that verifies Ethereum Mainnet's consensus.

[Docs](https://succinctlabs.github.io/sp1-helios/)

## Operator

By default, the operator commits finalized light-client state and the execution
state root:

```sh
cargo run -p sp1-helios-script --bin operator -- \
  --rpc-url "$DESTINATION_RPC_URL" \
  --contract-address "$SP1_HELIOS" \
  --source-chain-id "$SOURCE_CHAIN_ID" \
  --source-consensus-rpc "$SOURCE_CONSENSUS_RPC_URL" \
  --private-key "$DESTINATION_PRIVATE_KEY"
```

Receipt/log consumers also need the finalized execution block hash and receipts
root. Use `--commit-execution-header` for that path:

```sh
cargo run -p sp1-helios-script --bin operator -- \
  --rpc-url "$DESTINATION_RPC_URL" \
  --contract-address "$SP1_HELIOS" \
  --source-chain-id "$SOURCE_CHAIN_ID" \
  --source-consensus-rpc "$SOURCE_CONSENSUS_RPC_URL" \
  --private-key "$DESTINATION_PRIVATE_KEY" \
  --commit-execution-header
```

This writes extra on-chain storage for each update.
