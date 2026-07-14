//! Executor-based regression test for the `next_sync_committee` under-constraint fix
//! (GHSA-83q5-vwj7-gxww, shipped in v1.2.0 / commit 07096e1).
//!
//! Before the fix the light-client program committed the prover-supplied
//! `store.next_sync_committee` straight to `ProofOutputs.nextSyncCommitteeHash` without
//! re-deriving it from a verified update. With an empty `updates` list nothing constrained it,
//! so a prover could commit an arbitrary next sync committee that the contract stored on-chain.
//! The fix resets `store.next_sync_committee = None` immediately after deserializing the inputs,
//! so the committed hash is only ever populated by a `verify_update`-validated update.
//!
//! These tests run the canonical `elf/light_client` ELF through the SP1 *executor* (no proving)
//! against a committed real-mainnet `ProofInputs` fixture (see `bin/gen_fixture.rs`).

use alloy::primitives::B256;
use alloy::sol_types::SolType;
use sp1_helios_primitives::types::{ExecutionHeaderProofOutputs, ProofInputs, ProofOutputs};
use sp1_sdk::{Elf, Prover, ProverClient, SP1Stdin};
use tree_hash::TreeHash;

const LIGHT_CLIENT_ELF: &[u8] = include_bytes!("../../elf/light_client");
const EXECUTION_HEADER_ELF: &[u8] = include_bytes!("../../elf/execution_header");
const FIXTURE: &[u8] = include_bytes!("fixtures/proof_inputs.cbor");

/// Execute an ELF over the given CBOR-encoded `ProofInputs` and return the committed public values.
async fn execute_elf(elf: &'static [u8], cbor_inputs: &[u8]) -> Vec<u8> {
    let mut stdin = SP1Stdin::new();
    stdin.write_slice(cbor_inputs);

    let client = ProverClient::builder().cpu().build().await;
    let (public_values, _report) = client
        .execute(Elf::Static(elf), stdin)
        .await
        .expect("execution failed");

    public_values.to_vec()
}

/// Execute the light-client ELF over the given CBOR-encoded `ProofInputs` and decode the
/// committed `ProofOutputs`.
async fn execute_light_client(cbor_inputs: &[u8]) -> ProofOutputs {
    let public_values = execute_elf(LIGHT_CLIENT_ELF, cbor_inputs).await;
    ProofOutputs::abi_decode(public_values.as_slice()).expect("failed to decode ProofOutputs")
}

/// Execute the execution-header ELF over the given CBOR-encoded `ProofInputs` and decode the
/// committed `ExecutionHeaderProofOutputs`.
async fn execute_execution_header(cbor_inputs: &[u8]) -> ExecutionHeaderProofOutputs {
    let public_values = execute_elf(EXECUTION_HEADER_ELF, cbor_inputs).await;
    ExecutionHeaderProofOutputs::abi_decode(public_values.as_slice())
        .expect("failed to decode ExecutionHeaderProofOutputs")
}

/// NEGATIVE / regression guard: a prover supplies an empty `updates` list and a poisoned
/// `store.next_sync_committee` (here a clone of the trusted current committee, standing in for
/// any attacker-chosen committee). The honest current committee still verifies the finality
/// update, so the program runs to completion — but the fix drops the poisoned next committee, so
/// the committed `nextSyncCommitteeHash` must be `B256::ZERO`.
///
/// Without the fix this would commit `tree_hash_root()` of the poisoned committee instead.
#[tokio::test]
async fn executor_regressions() {
    next_sync_committee_poisoning_is_dropped().await;
    verified_next_sync_committee_is_committed().await;
    execution_header_drops_poisoned_next_sync_committee().await;
}

async fn next_sync_committee_poisoning_is_dropped() {
    let mut inputs: ProofInputs =
        serde_cbor::from_slice(FIXTURE).expect("failed to deserialize fixture");

    // Empty the updates so nothing legitimately populates `next_sync_committee` in-circuit.
    inputs.updates = vec![];
    // Poison the prover-controlled next committee. Cloning the current committee keeps the test
    // self-contained while standing in for arbitrary attacker-chosen keys.
    inputs.store.next_sync_committee = Some(inputs.store.current_sync_committee.clone());

    let cbor = serde_cbor::to_vec(&inputs).expect("failed to reserialize inputs");
    let outputs = execute_light_client(&cbor).await;

    println!(
        "negative: nextSyncCommitteeHash = {}",
        outputs.nextSyncCommitteeHash
    );
    assert_eq!(
        outputs.nextSyncCommitteeHash,
        B256::ZERO,
        "poisoned next_sync_committee leaked into the committed output; the fix is not in effect"
    );
}

/// POSITIVE / liveness: the unmodified fixture's `updates` legitimately populate
/// `next_sync_committee` via `verify_update`/`apply_update`, so the committed hash must be
/// non-zero. This proves the fix does not break the honest update path.
async fn verified_next_sync_committee_is_committed() {
    let inputs: ProofInputs =
        serde_cbor::from_slice(FIXTURE).expect("failed to deserialize fixture");
    assert!(
        !inputs.updates.is_empty(),
        "fixture has no updates; cannot exercise the honest next_sync_committee path"
    );

    // The fixture's updates all sit in the store's current period, so the last update's
    // `next_sync_committee` is the one `apply_update` commits. Derive the expected hash from it.
    let expected: B256 = inputs
        .updates
        .last()
        .unwrap()
        .next_sync_committee()
        .tree_hash_root();

    let outputs = execute_light_client(FIXTURE).await;

    println!(
        "positive: nextSyncCommitteeHash = {} (expected {expected})",
        outputs.nextSyncCommitteeHash
    );
    assert_ne!(
        outputs.nextSyncCommitteeHash,
        B256::ZERO,
        "honest next_sync_committee was dropped; the fix is too aggressive"
    );
    assert_eq!(
        outputs.nextSyncCommitteeHash, expected,
        "committed next_sync_committee hash does not match the verified update's committee"
    );
}

async fn execution_header_drops_poisoned_next_sync_committee() {
    let mut inputs: ProofInputs =
        serde_cbor::from_slice(FIXTURE).expect("failed to deserialize fixture");

    inputs.updates = vec![];
    inputs.store.next_sync_committee = Some(inputs.store.current_sync_committee.clone());

    let cbor = serde_cbor::to_vec(&inputs).expect("failed to reserialize inputs");
    let outputs = execute_execution_header(&cbor).await;

    assert_eq!(
        outputs.nextSyncCommitteeHash,
        B256::ZERO,
        "poisoned next_sync_committee leaked through the execution-header program"
    );
    assert_ne!(
        outputs.executionBlockHash,
        B256::ZERO,
        "execution-header output did not commit an execution block hash"
    );
    assert_ne!(
        outputs.executionReceiptsRoot,
        B256::ZERO,
        "execution-header output did not commit a receipts root"
    );
}
