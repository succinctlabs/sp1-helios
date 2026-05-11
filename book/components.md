# Components

An SP1 Helios deployment has four moving parts.

## `SP1Helios` contract

Lives on the destination chain. Stores the latest finalized beacon header,
execution state root, sync committee hash per period, and any attested
storage slots. Holds two verification keys — `lightClientVkey` for the
consensus update program and `storageSlotVkey` for the standalone storage
proof program — plus the address of an SP1 verifier contract. A `guardian`
address can rotate either vkey (and the guardian itself can be relinquished).

Exposes two entrypoints:
- `update(...)` — applies a consensus update produced by the `light_client`
  guest program, advancing `head`, `executionStateRoot`, and the sync
  committee mapping, and optionally writing storage slots bundled with the
  update.
- `updateStorageSlot(...)` — verifies a standalone storage proof produced
  by the `storage` guest program against a state root the contract already
  trusts, and writes the slots.

## SP1 verifier contract

The contract address stored in `SP1Helios.verifier` — verifies any SP1
proof against a given vkey and public values. The canonical SP1 PLONK
verifier addresses per chain are listed in the [SP1 contract addresses
docs](https://docs.succinct.xyz/docs/sp1/verification/contract-addresses).
Deployers can substitute their own deployment or `SP1MockVerifier` for
local testing.

## SP1 Helios guest programs

Two programs ship in `program/`:
- `light_client` — runs Helios consensus verification (sync-committee
  signature check, finality proof, sync-committee-rotation proof, header
  validity) and, in the same proof, verifies any requested source-chain
  storage slots against the new execution state root. Commits a
  `ProofOutputs` struct as public values.
- `storage` — verifies storage slot MPT proofs against a state root passed
  in as input. Used for ad-hoc storage proofs after a state root is
  already trusted by the contract. Commits a `StorageProofOutputs` struct.

## Operator

A Rust binary (`script/bin/operator.rs`) that fetches the contract's
current head, calls a beacon node to assemble the next consensus update,
runs the `light_client` guest, and submits the resulting proof to
`SP1Helios.update`. Runs on a configurable interval (default 5 minutes).
