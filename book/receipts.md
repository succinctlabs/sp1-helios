# Receipts Route (Planned)

The current programs attest **state** — the execution state root and any
MPT-verified storage slot values. State is the right primitive for
balances, slot reads, and any value with on-chain canonical storage.

A second, parallel route for **receipts** is planned to support
event-driven use cases (e.g. proving a log was emitted by a specific
contract at a specific block on the source chain). The pattern mirrors
the existing storage route:

1. Each canonical execution-payload header commits to a `receipts_root`
   alongside `state_root`. A future `light_client` revision would
   commit `receiptsRoot` into `ProofOutputs` so the `SP1Helios`
   contract anchors it the same way it currently anchors
   `executionStateRoot`.
2. A `receipts` guest program (counterpart to the existing `storage`
   program) would verify MPT inclusion proofs of a receipt against
   the trusted `receiptsRoot`, then verify any specific log within
   the receipt's log list.

Composition details — bundling vs. standalone proofs, batch shapes,
ABI for `ReceiptProofOutputs` — are not finalized. This section will be
expanded once the design lands. Reach out if you have a concrete use
case that would help inform it.
