//! Generate the `ProofInputs` fixture used by the light-client executor regression test.
//!
//! Mirrors `operator::request_update` / `lib::{get_client, get_updates}`: bootstrap a helios
//! client a few sync-committee periods behind the latest finalized checkpoint, fetch the
//! committee `updates` and the latest `finality_update`, and serialize the resulting
//! `ProofInputs` (with no contract storage) to CBOR.
//!
//! Bootstrapping behind the latest finalized slot guarantees the finality update advances the
//! head (`newHead > prevHead`) and that `updates` legitimately populates `next_sync_committee`,
//! so the same fixture exercises both the positive and negative paths of `tests/execute.rs`.
//!
//! Run against a mainnet beacon API, e.g.:
//!   cargo run --release --bin gen_fixture -- \
//!     --source-consensus-rpc https://ethereum-beacon-api.publicnode.com \
//!     --output script/tests/fixtures/proof_inputs.cbor
use anyhow::{anyhow, Result};
use clap::Parser;
use helios_consensus_core::consensus_spec::MainnetConsensusSpec;
use helios_consensus_core::types::BeaconBlock;
use helios_ethereum::rpc::ConsensusRpc;
use sp1_helios_primitives::types::ProofInputs;
use sp1_helios_script::{get_client, get_updates};
use std::path::PathBuf;
use tree_hash::TreeHash;

const SLOTS_PER_PERIOD: u64 = 8192;
/// Bootstrap this many checkpoint slots (32 slots each) behind the finality update's head, so the
/// finality update advances the head while staying within the same sync-committee period.
const CHECKPOINTS_BEHIND: u64 = 16;

fn sync_period(slot: u64) -> u64 {
    slot / SLOTS_PER_PERIOD
}

#[derive(Parser, Debug)]
#[command(about = "Generate the ProofInputs CBOR fixture for the light-client executor test.")]
struct Args {
    /// Mainnet beacon (consensus) RPC serving the helios light-client API.
    #[arg(long)]
    source_consensus_rpc: String,

    /// Source chain id (mainnet = 1).
    #[arg(long, default_value = "1")]
    source_chain_id: u64,

    /// Output path for the CBOR fixture.
    #[arg(long, default_value = "script/tests/fixtures/proof_inputs.cbor")]
    output: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // 1. Bootstrap at the latest checkpoint just to talk to the RPC, and read the live finality
    //    update so we know the head it finalizes and the sync-committee period it signs against.
    let latest = get_client(None, &args.source_consensus_rpc, args.source_chain_id).await?;
    let finality_update = latest
        .rpc
        .get_finality_update()
        .await
        .map_err(|e| anyhow!("failed to get finality update: {e}"))?;
    let new_head = finality_update.finalized_header().beacon().slot;
    let finality_period = sync_period(*finality_update.signature_slot());
    println!(
        "finality update: new head {new_head} (period {}), signature period {finality_period}",
        sync_period(new_head)
    );

    // We want the bootstrap head strictly behind `new_head` so the finality update advances the
    // head, but in the SAME sync-committee period as the finality update's signature. Keeping
    // them in one period means an empty `updates` list still leaves the finality update
    // verifiable against the (unchanged) current committee — the exact condition the negative
    // regression test relies on — while `get_updates` for that period still carries the next
    // committee for the positive test.
    let mut bootstrap_slot = new_head.saturating_sub(CHECKPOINTS_BEHIND * 32) / 32 * 32;

    // Step back by 32-slot checkpoints until we hit a slot that has a real block and is still in
    // the finality update's period (period boundaries can be skipped slots).
    loop {
        assert_eq!(
            sync_period(bootstrap_slot),
            finality_period,
            "ran out of in-period checkpoints with a block (slot {bootstrap_slot})"
        );
        match latest.rpc.get_block(bootstrap_slot).await {
            Ok(block) => {
                let block: BeaconBlock<MainnetConsensusSpec> = block;
                let _ = block.tree_hash_root();
                break;
            }
            Err(e) => {
                println!("no block at slot {bootstrap_slot} ({e}); stepping back 32 slots");
                bootstrap_slot = bootstrap_slot
                    .checked_sub(32)
                    .ok_or_else(|| anyhow!("ran out of slots searching for a checkpoint block"))?;
            }
        }
    }

    // 2. Bootstrap the client at that checkpoint, exactly like the operator does.
    let client = get_client(
        Some(bootstrap_slot),
        &args.source_consensus_rpc,
        args.source_chain_id,
    )
    .await?;
    assert_eq!(
        client.store.finalized_header.beacon().slot,
        bootstrap_slot,
        "bootstrapped client has mismatched finalized slot"
    );

    // 3. Fetch committee updates (mirrors request_update). These populate next_sync_committee in
    //    the positive test; the negative test empties them.
    let updates = get_updates(&client).await;

    println!(
        "bootstrap slot: {bootstrap_slot} (period {}), updates: {}, finality new head: {new_head}",
        sync_period(bootstrap_slot),
        updates.len()
    );
    assert!(
        new_head > bootstrap_slot,
        "finality update does not advance head ({new_head} <= {bootstrap_slot})"
    );
    assert!(
        new_head.is_multiple_of(32),
        "finality update head {new_head} is not a checkpoint slot"
    );
    assert!(
        !updates.is_empty(),
        "no committee updates returned; positive test cannot exercise next_sync_committee"
    );

    // 4. Build ProofInputs with no contract storage (keeps the fixture small).
    let inputs = ProofInputs {
        updates,
        finality_update,
        expected_current_slot: client.expected_current_slot(),
        store: client.store.clone(),
        genesis_root: client.config.chain.genesis_root,
        forks: client.config.forks.clone(),
        contract_storage: vec![],
    };

    let encoded = serde_cbor::to_vec(&inputs)?;
    if let Some(parent) = args.output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&args.output, &encoded)?;
    println!("wrote {} bytes to {}", encoded.len(), args.output.display());

    Ok(())
}
