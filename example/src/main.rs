#![no_std]

extern crate alloc;

use alloc::vec::Vec;

use eyre::Result;

use alloy_primitives::B256;
use consensus_core::types::{
    FinalityUpdate, Forks, LightClientStore, SyncAggregate, SyncCommittee, Update,
};
use consensus_core::{
    apply_finality_update, apply_update, types::Header, verify_finality_update, verify_update,
};

/// Program flow:
/// 1. Apply sync committee updates, if any
/// 2. Apply finality update
/// 3. Verify execution state root proof
/// 4. Asset all updates are valid
/// 5. Commit new state root, header, and sync committee for usage in the on-chain contract
pub fn main() -> Result<()> {
    let mut lc_store = LightClientStore::default();
    let finality_update = FinalityUpdate {
        finalized_header: Header::default(),
        finality_branch: Vec::new(),
        attested_header: Header::default(),
        sync_aggregate: SyncAggregate::default(),
        signature_slot: 0,
    };
    let update = Update {
        attested_header: Header::default(),
        next_sync_committee: SyncCommittee::default(),
        sync_aggregate: SyncAggregate::default(),
        next_sync_committee_branch: Vec::new(),
        finalized_header: Header::default(),
        finality_branch: Vec::new(),
        signature_slot: 0,
    };
    apply_finality_update(&mut lc_store, &finality_update);
    apply_update(&mut lc_store, &update);

    verify_finality_update(
        &finality_update,
        0,
        &lc_store,
        B256::default(),
        &Forks::default(),
    )?;
    verify_update(&update, 0, &lc_store, B256::default(), &Forks::default())?;
    Ok(())
}
