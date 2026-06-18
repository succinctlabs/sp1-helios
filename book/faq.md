# Frequently Asked Questions

## How do I prove data on the source chain against an SP1 Helios light client?

After an `update(...)` lands on the destination contract, the source
chain's execution state root for that block is available at
`SP1Helios.executionStateRoots(blockNumber)`. Use standard Ethereum
[Merkle Patricia Trie](https://ethereum.org/en/developers/docs/data-structures-and-encoding/patricia-merkle-trie/)
proofs against that root to prove an account's `TrieAccount` and then any
storage slot value. See the [Trust Model](./trust-model.md) chapter for
the full chain of trust, and `program/src/storage.rs` for a reference
implementation.

## Where in Helios is the consensus verification?

The free functions `verify_update` / `apply_update` and
`verify_finality_update` / `apply_finality_update` in
[`helios-consensus-core`](https://github.com/a16z/helios/blob/master/ethereum/consensus-core/src/consensus_core.rs)
are the entrypoints the SP1 Helios guest calls. They run sync-committee
signature verification, finality and sync-committee-rotation Merkle
proofs, and header validity checks.

## Which source chains are supported?

The current ELFs target Ethereum mainnet (`MainnetConsensusSpec` and
chain ID 1 are the defaults). The guest is generic over Helios's
`ConsensusSpec` trait, so other beacon-chain networks (e.g. testnets)
work in principle but require building new ELFs and re-deriving vkeys.

## Can I have multiple `SP1Helios` instances tracking different source chains?

Yes — deploy one contract per source chain, each with its own
`sourceChainId`, initial checkpoint, and (if you've built different
guest ELFs) vkeys.

## What happens if the operator goes offline?

The contract stops advancing but remains consistent. When the operator
returns, it generates a proof spanning the gap. The 32-slot
checkpoint-boundary requirement means the operator should reorganize
its request to land on a checkpoint slot; the guest already enforces
this.
