use crate::errors::ConsensusError;
use crate::types::{
    ByteVector, Bytes32, ChainConfig, GenericUpdate, Header, LightClientStore, SyncCommittee,
    Update,
};
use std::sync::Arc;
use std::time::Duration;
use ssz_rs::prelude::*;

pub fn verify_update(
    update: &Update,
    store: LightClientStore,
    config: ChainConfig,
    now: Duration,
) -> Result<()> {
    let update = GenericUpdate::from(update);
    verify_generic_update(&update, store, config, now)
}

// implements checks from validate_light_client_update and process_light_client_update in the
// specification
fn verify_generic_update(
    update: &GenericUpdate,
    store: LightClientStore,
    config: ChainConfig,
    now: Duration,
) -> Result<()> {
    let bits = get_bits(&update.sync_aggregate.sync_committee_bits);
    if bits == 0 {
        return Err(ConsensusError::InsufficientParticipation.into());
    }

    let update_finalized_slot = update.finalized_header.clone().unwrap_or_default().slot;
    let valid_time = expected_current_slot(now, config.genesis_time) >= update.signature_slot
        && update.signature_slot > update.attested_header.slot.as_u64()
        && update.attested_header.slot >= update_finalized_slot;

    if !valid_time {
        return Err(ConsensusError::InvalidTimestamp.into());
    }

    let store_period = calc_sync_period(store.finalized_header.slot.into());
    let update_sig_period = calc_sync_period(update.signature_slot);
    let valid_period = if store.next_sync_committee.is_some() {
        update_sig_period == store_period || update_sig_period == store_period + 1
    } else {
        update_sig_period == store_period
    };

    if !valid_period {
        return Err(ConsensusError::InvalidPeriod.into());
    }

    let update_attested_period = calc_sync_period(update.attested_header.slot.into());
    let update_has_next_committee = store.next_sync_committee.is_none()
        && update.next_sync_committee.is_some()
        && update_attested_period == store_period;

    if update.attested_header.slot <= store.finalized_header.slot && !update_has_next_committee {
        return Err(ConsensusError::NotRelevant.into());
    }

    if update.finalized_header.is_some() && update.finality_branch.is_some() {
        let is_valid = is_finality_proof_valid(
            &update.attested_header,
            &mut update.finalized_header.clone().unwrap(),
            &update.finality_branch.clone().unwrap(),
        );

        if !is_valid {
            return Err(ConsensusError::InvalidFinalityProof.into());
        }
    }

    if update.next_sync_committee.is_some() && update.next_sync_committee_branch.is_some() {
        let is_valid = is_next_committee_proof_valid(
            &update.attested_header,
            &mut update.next_sync_committee.clone().unwrap(),
            &update.next_sync_committee_branch.clone().unwrap(),
        );

        if !is_valid {
            return Err(ConsensusError::InvalidNextSyncCommitteeProof.into());
        }
    }

    let sync_committee = if update_sig_period == store_period {
        &self.store.current_sync_committee
    } else {
        self.store.next_sync_committee.as_ref().unwrap()
    };

    let pks = get_participating_keys(sync_committee, &update.sync_aggregate.sync_committee_bits)?;

    let is_valid_sig = self.verify_sync_committee_signture(
        &pks,
        &update.attested_header,
        &update.sync_aggregate.sync_committee_signature,
        update.signature_slot,
    );

    if !is_valid_sig {
        return Err(ConsensusError::InvalidSignature.into());
    }

    Ok(())
}

pub fn calc_sync_period(slot: u64) -> u64 {
    let epoch = slot / 32; // 32 slots per epoch
    epoch / 256 // 256 epochs per sync committee
}

fn expected_current_slot(now: Duration, genesis_time: u64) -> u64 {
    let since_genesis = now - std::time::Duration::from_secs(genesis_time);

    since_genesis.as_secs() / 12
}

fn get_bits(bitfield: &Bitvector<512>) -> u64 {
    let mut count = 0;
    bitfield.iter().for_each(|bit| {
        if bit == true {
            count += 1;
        }
    });

    count
}

fn is_finality_proof_valid(
    attested_header: &Header,
    finality_header: &mut Header,
    finality_branch: &[Bytes32],
) -> bool {
    is_proof_valid(attested_header, finality_header, finality_branch, 6, 41)
}

pub fn is_proof_valid<L: Merkleized>(
    attested_header: &Header,
    leaf_object: &mut L,
    branch: &[Bytes32],
    depth: usize,
    index: usize,
) -> bool {
    let res: Result<bool> = (move || {
        let leaf_hash = leaf_object.hash_tree_root()?;
        let state_root = bytes32_to_node(&attested_header.state_root)?;
        let branch = branch_to_nodes(branch.to_vec())?;

        let is_valid = is_valid_merkle_branch(&leaf_hash, branch.iter(), depth, index, &state_root);
        Ok(is_valid)
    })();

    if let Ok(is_valid) = res {
        is_valid
    } else {
        false
    }
}

pub fn branch_to_nodes(branch: Vec<Bytes32>) -> Result<Vec<Node>> {
    branch
        .iter()
        .map(bytes32_to_node)
        .collect::<Result<Vec<Node>>>()
}

pub fn bytes32_to_node(bytes: &Bytes32) -> Result<Node> {
    Ok(Node::try_from(bytes.as_slice())?)
}
