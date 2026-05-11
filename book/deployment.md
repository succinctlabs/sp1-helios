# Deployment

## Overview

SP1 Helios verifies the consensus of a source chain in the execution
environment of a destination chain. For example, you can run SP1 Helios on
Polygon that verifies Ethereum Mainnet's consensus.

## Prerequisites

- [Foundry](https://getfoundry.sh/)
- [SP1](https://docs.succinct.xyz/docs/sp1/getting-started/install)
- A funded deployer key (private key or Ledger) on the destination chain

## 1. Consensus RPC

The deployment requires a beacon-chain RPC for the source chain that
supports the [Altair light-client sync protocol](https://github.com/ethereum/consensus-specs/blob/master/specs/altair/light-client/sync-protocol.md)
endpoints (`/eth/v1/beacon/light_client/*`). Not all consensus clients
expose these by default — Nimbus does; other clients may need a CLI flag
or a recent enough version. Options:

1. A managed provider that exposes the light-sync endpoints (e.g.
   [Chainstack](https://chainstack.com/) Ethereum beacon nodes).
2. Run your own node configured to expose the light-sync endpoints.
3. A community list of public endpoints lives at
   <https://s1na.github.io/light-sync-endpoints>; these are best-effort
   and not recommended for production.

The chosen URL is `SOURCE_CONSENSUS_RPC` in the next steps.

## 2. Deploy the contract

`bin/genesis` queries the consensus RPC, builds an initial trusted
checkpoint (`genesis.json`), and invokes `forge script` to deploy
`SP1Helios` with those parameters baked in. The PLONK verifier address
for your destination chain is in the [SP1 contract addresses
docs](https://docs.succinct.xyz/docs/sp1/verification/contract-addresses).

```bash
cargo run --bin genesis -- \
  --rpc-url <DESTINATION_RPC_URL> \
  --source-consensus-rpc <SOURCE_CONSENSUS_RPC> \
  --source-chain-id 1 \
  --sp1-verifier-address <SP1_VERIFIER_ADDRESS> \
  --guardian-address <GUARDIAN_ADDRESS> \
  --private-key <DEPLOYER_PK>
  # or: --ledger [--ledger-path N]
  # optional: --slot <SLOT> --etherscan-api-key <KEY>
```

The forge broadcast prints the deployed `SP1Helios` address.

## 3. Run the operator

The operator polls `SP1Helios.head`, fetches updates from the consensus
RPC, generates a proof, and submits `update(...)`:

```bash
cargo run --release --bin operator -- \
  --rpc-url <DESTINATION_RPC_URL> \
  --contract-address <SP1HELIOS_ADDRESS> \
  --source-chain-id 1 \
  --source-consensus-rpc <SOURCE_CONSENSUS_RPC> \
  --private-key <RELAYER_PK> \
  --loop-delay-mins 5
```

Proving uses [`sp1_sdk::EnvProver`](https://docs.rs/sp1-sdk/), which reads
prover configuration (network vs. local, network endpoint, keys, etc.)
from environment variables — see the SP1 SDK docs for the available knobs.

Sample log output:

```
INFO operator: Starting SP1 Helios operator
INFO operator: Updating to new head block: 6107712 from 6107648
INFO operator: Successfully updated to new head block! Tx hash: 0x4a0d...
```
