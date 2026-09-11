# SP1 Helios

## Overview

SP1 Helios verifies the consensus of a source chain in the execution environment of a destination chain. For example, you can run an SP1 Helios light client on Polygon that verifies Ethereum Mainnet's consensus.

[Docs](https://succinctlabs.github.io/sp1-helios/)

## Operator

The operator keeps an on-chain SP1 Helios light client updated by proving finalized source-chain consensus updates and submitting them to the destination-chain SP1 Helios contract.

Proof requests are fulfilled through the [Succinct Prover Network](https://docs.succinct.xyz/docs/sp1/prover-network/quickstart). See `.env.example` for additional configuration.

The operator supports two commitment modes: (1) default mode and (2) execution-header mode.

### Docker

Build the operator from the repository root:

```sh
docker build --platform linux/amd64 -t sp1-helios:operator .
docker run --rm sp1-helios:operator operator --help
```

The image embeds the checked-in ELFs and runs as user `10001:10001`.
It uses the Prover Network and needs outbound access to the configured RPC endpoints.

Pass configuration at runtime:

```sh
docker run --rm --env-file .env sp1-helios:operator operator \
  --rpc-url "$DESTINATION_RPC_URL" \
  --contract-address "$SP1_HELIOS" \
  --source-chain-id "$SOURCE_CHAIN_ID" \
  --source-consensus-rpc "$SOURCE_CONSENSUS_RPC_URL" \
  --private-key "$DESTINATION_PRIVATE_KEY"
```

Add `--commit-execution-header` for execution-header mode.

The Docker workflow builds pull requests without publishing.
Pushes and manual runs publish Linux AMD64 images to `ghcr.io/<owner>/sp1-helios`.
Tags use `operator-<short-sha>`, `operator-<git-tag>`, and `operator-latest` for `main`.
Use a commit tag or image digest for deployments.

For ECS, set the container command to `["operator", "--rpc-url", "...", ...]`.
Supply the Prover Network settings through the task environment or secrets.
Docker sends SIGINT when it stops this image, which matches the operator's shutdown handler.

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
