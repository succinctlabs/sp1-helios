# SP1 Helios

## Overview

SP1 Helios is a ZK light client for Ethereum's beacon chain. It runs the
[Helios](https://github.com/a16z/helios) consensus verification logic inside
an [SP1](https://github.com/succinctlabs/sp1) zkVM program, producing a
succinct proof that a given beacon block header — and the execution state
root committed inside it — was attested by the Altair sync committee.

A deployed `SP1Helios` contract on any EVM destination chain consumes these
proofs to maintain an up-to-date view of the source chain's finalized
headers, execution state roots, and sync committees. Downstream contracts
can use the attested execution state root to verify source-chain storage
via standard MPT proofs.

## Repository layout

- `program/` — the SP1 guest programs (`light_client` and `storage`).
- `primitives/` — shared types (`ProofInputs`, `ProofOutputs`,
  `StorageProofOutputs`, `ContractStorage`) and the MPT verification helper.
- `script/` — host binaries: `genesis` (deploy-time bootstrap),
  `operator` (proof generation loop), `vkey` (vkey inspection).
- `contracts/` — `SP1Helios.sol` and its Foundry deploy script.
- `elf/` — built guest ELFs consumed by the host binaries via
  `include_bytes!`.
