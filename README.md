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
  --source-execution-rpc "$SOURCE_EXECUTION_RPC_URL" \
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
  --source-execution-rpc "$SOURCE_EXECUTION_RPC_URL" \
  --private-key "$DESTINATION_PRIVATE_KEY" \
  --commit-execution-header
```

### Docker

See the [deployment guide](book/deployment.md) for source RPC requirements and program key updates.

The CI publishes Linux AMD64 images as `ghcr.io/succinctlabs/sp1-helios:operator-<short-sha>`.
To build locally and view the operator flags:

```sh
docker build --platform linux/amd64 -t sp1-helios:operator .
docker run --rm sp1-helios:operator operator --help
```

## Validation

The recorded Plataberget fixtures contain signed Gloas updates, execution headers, and source storage proofs.
`gloas_transition.cbor` starts at slot 49120 and advances to slot 56416 across the Gloas fork and a committee change.
The historical finality update retains the signature and proof branches from the last recorded committee update.
`gloas_current.cbor` advances from slot 379968 to slot 380480 after Gloas.

Run the unit and guest execution tests with the committed ELFs:

```sh
SP1_SKIP_PROGRAM_BUILD=true cargo test --locked --release \
  -p sp1-helios-primitives -p sp1-helios-script \
  --lib --test execute --test storage_collection --test gloas
```

The storage tests require Anvil.
These tests execute the programs without generating proofs.
Generate a real local PLONK proof with the CPU prover:

```sh
RUST_LOG=info SP1_SKIP_PROGRAM_BUILD=true cargo run --locked --release \
  -p sp1-helios-script --bin validate_fixture -- \
  --input script/tests/fixtures/gloas_transition.cbor \
  --mode light-client --prove \
  --output contracts/validation/proofs/transition-light-client.json
```

Use `--mode execution-header` for the other update program.
Remove `--prove` to execute only.
CUDA proving requires the `cuda` Cargo feature and the `--cuda` flag.
The local prover does not submit requests to the Prover Network.
Set `SP1_PLONK_CIRCUIT_PATH` to a writable directory if the default circuit cache is read-only.

Verify the generated proof through the real SP1 verifier and SP1 Helios contract:

```sh
cd contracts
FOUNDRY_PROFILE=proof \
  SP1_HELIOS_PROOF_PATH=validation/proofs/transition-light-client.json \
  forge test --match-contract RealProofTest -vv
```

This test checks stored outputs and rejects changed proofs, public values, wrong program keys, and replayed updates.
Default Foundry tests use a mock verifier.
Real proof validation requires the separate `proof` profile and a generated proof file.
