# Trust Model

This section describes, end-to-end, what an SP1 Helios deployment trusts
and why. The goal is to make explicit what an integrator can take as given
when they read a value off the contract.

The chain of trust has four links:

1. A deploy-time trusted checkpoint that seeds the contract.
2. An SP1 proof, over every update, that attests a new finalized header
   was correctly derived from the previous trusted state.
3. The on-chain contract pinning the proof's public values to its own
   storage so the proof can only verify against the state the contract
   already believes.
4. Standard MPT verification against the attested execution state root
   for any downstream storage access.

## 1. Deploy-time trusted checkpoint

`SP1Helios` is constructed with an `InitParams` struct (see
`contracts/src/SP1Helios.sol`) that fixes:

- `head`, `header`, `executionStateRoot`, `executionBlockNumber`
- `syncCommitteeHash` for the period containing `head`
- `lightClientVkey`, `storageSlotVkey`
- The SP1 `verifier` address and `guardian`

These values are produced by `bin/genesis`, which queries a beacon RPC
for a recent finalized checkpoint and writes `genesis.json` for the
forge deploy script. **Whoever deploys is trusted to seed an honest
initial state.** Once seeded, every subsequent update is verified by
zero-knowledge proof — no further off-chain trust is required from the
operator running the relayer.

The `guardian` retains the ability to rotate `lightClientVkey` and
`storageSlotVkey` (e.g. to upgrade the guest program). The guardian can
also relinquish itself, making the deployment immutable.

## 2. What the SP1 proof attests on each update

The `light_client` guest program runs the Helios consensus verification
logic against inputs supplied by the operator and commits a
`ProofOutputs` struct (`primitives/src/types.rs`) as public values:

| Field                   | Meaning                                                   |
| ----------------------- | --------------------------------------------------------- |
| `prevHeader`            | The beacon header the proof was built **from**            |
| `prevHead`              | The slot of that previous header                          |
| `prevSyncCommitteeHash` | The sync committee that signed the chain of updates       |
| `newHeader`             | The new finalized beacon header                           |
| `newHead`               | The slot of the new header (must be a multiple of 32)     |
| `executionStateRoot`    | The execution state root committed inside the new header  |
| `executionBlockNumber`  | The execution block number for that state root            |
| `syncCommitteeHash`     | The sync committee hash for the new period                |
| `nextSyncCommitteeHash` | The next-period sync committee hash, if rotated           |
| `storageSlots`          | Any storage slots requested by the operator, MPT-verified against `executionStateRoot` in the same proof |

The guest, in `program/src/light_client.rs`, applies each
`Update`/`FinalityUpdate` in turn via `verify_update` / `apply_update`
from `helios-consensus-core`, asserts the new head is strictly greater
than the previous head and lies on a 32-slot checkpoint boundary, then
verifies each requested storage slot's MPT proof against the new
execution state root before committing the `ProofOutputs`.

## 3. How the contract anchors trust

`SP1Helios.update` does not accept the previous-state fields from the
caller. It reads them from its own storage:

```solidity
ProofOutputs memory po = ProofOutputs({
    prevHeader: headers[head],
    prevHead: head,
    prevSyncCommitteeHash: syncCommittees[getSyncCommitteePeriod(head)],
    newHead: newHead,
    newHeader: newHeader,
    /* ... */
});
ISP1Verifier(verifier).verifyProof(lightClientVkey, abi.encode(po), proof);
```

The proof can only verify if the public values it was generated over
match this exact struct byte-for-byte. So:

- A proof built from an attacker-chosen previous header cannot verify —
  the contract would have substituted its own `headers[head]`.
- The previous sync-committee hash used inside the guest is pinned to
  whatever the contract last stored. The guest's signature check
  therefore runs against a committee the contract already trusts.

After the proof verifies, the contract writes the new head, header,
state root, sync committees, and storage slots. Sync committees for
future periods, if already set, must match (`NextSyncCommitteeMismatch`
revert) — preventing inconsistent rotations.

The standalone `updateStorageSlot` entrypoint uses the same pattern:
the storage root is filled in from `executionStateRoots[blockNumber]`,
which the contract already trusts because it was committed by a prior
verified `light_client` proof. A caller cannot supply their own root.

## 4. Downstream storage verification

Once `SP1Helios.executionStateRoots[blockNumber]` is populated, any
contract on the destination chain can prove a value at any source-chain
storage slot at that block by passing standard MPT proofs through the
`storage` program (or by re-implementing the same check off-chain and
re-verifying inside a different SP1 program). The mechanics mirror
`program/src/storage.rs`:

1. Hash the contract address with `keccak256` and verify a state-trie
   proof against `executionStateRoot`, yielding the contract's
   `TrieAccount` (and therefore its `storage_root`).
2. For each slot key, hash with `keccak256` and verify a storage-trie
   proof against `storage_root`, yielding the slot's value.

These are the canonical Ethereum MPT layouts; no SP1-specific format
is involved. The only trusted input is `executionStateRoot`, which the
SP1 proof attests came from a sync-committee-signed beacon header.

## Why each link is trustless

| Link                                                  | Trust assumption                                              |
| ----------------------------------------------------- | ------------------------------------------------------------- |
| Deploy-time checkpoint → trusted initial state        | Deployer (verifiable against any beacon node at deploy time)  |
| Initial state → every later state                     | Soundness of SP1, soundness of Helios consensus verification, 2/3 of source-chain sync committee honest (the standard Altair assumption) |
| Contract storage → proof public values                | EVM execution and SP1 verifier correctness                    |
| `executionStateRoot` → source-chain storage slot value | Ethereum MPT inclusion / `keccak256` collision resistance     |

The relayer that runs `bin/operator` is **not** trusted: it cannot
forge a proof that verifies against the contract's pinned previous state,
and it cannot censor the chain indefinitely without becoming observable
(no head advancement). The only trust placed in any off-chain party is
the deployer's choice of initial checkpoint and the guardian's vkey
rotation authority — both of which are explicit and inspectable on chain.
