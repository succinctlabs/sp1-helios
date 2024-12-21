# Frequently Asked Questions

## How do I prove data on Ethereum against an SP1 Helios light client?

The SP1 Helios light client validates the Ethereum beacon chain's headers are signed correctly by
the Altair sync committee. The [Helios library](https://github.com/a16z/helios) verifies that the execution payload in the beacon chain's header is correct [here](https://github.com/a16z/helios/blob/8cd29787857303c6f455c08e948a694cc2e8f46d/ethereum/consensus-core/src/consensus_core.rs#L481-L507).

You can use storage proofs to Merkle prove data in the Merkle Patricia Trie against the execution state root.

