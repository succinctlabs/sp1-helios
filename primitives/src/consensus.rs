use crate::config::Config;
use crate::errors::ConsensusError;
use crate::types::{
    ByteVector, Bytes32, ChainConfig, ForkData, GenericUpdate, Header, LightClientStore,
    SignatureBytes, SigningData, SyncCommittee, Update,
};
use ssz_rs::prelude::*;
use std::result::Result;
use std::sync::Arc;
use std::time::Duration;

pub fn verify_update(
    update: &Update,
    store: LightClientStore,
    config: Config,
    now: Duration,
) -> Result<(), ConsensusError> {
    let update = GenericUpdate::from(update);
    verify_generic_update(&update, store, config, now)
}
use milagro_bls::{AggregateSignature, PublicKey};

// implements checks from validate_light_client_update and process_light_client_update in the
// specification
fn verify_generic_update(
    update: &GenericUpdate,
    store: LightClientStore,
    config: Config,
    now: Duration,
) -> Result<(), ConsensusError> {
    let bits = get_bits(&update.sync_aggregate.sync_committee_bits);
    if bits == 0 {
        return Err(ConsensusError::InsufficientParticipation.into());
    }

    let update_finalized_slot = update.finalized_header.clone().unwrap_or_default().slot;
    let valid_time = expected_current_slot(now, config.chain.genesis_time) >= update.signature_slot
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
        &store.current_sync_committee
    } else {
        store.next_sync_committee.as_ref().unwrap()
    };

    let pks = get_participating_keys(sync_committee, &update.sync_aggregate.sync_committee_bits)?;

    let is_valid_sig = verify_sync_committee_signture(
        &pks,
        &update.attested_header,
        &update.sync_aggregate.sync_committee_signature,
        update.signature_slot,
        &config,
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
    let res: Result<bool, ConsensusError> = (move || {
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

pub fn branch_to_nodes(branch: Vec<Bytes32>) -> Result<Vec<Node>, ConsensusError> {
    branch
        .iter()
        .map(bytes32_to_node)
        .collect::<Result<Vec<Node>, ConsensusError>>()
}

pub fn bytes32_to_node(bytes: &Bytes32) -> Result<Node, ConsensusError> {
    Ok(Node::try_from(bytes.as_slice()).unwrap())
}

fn is_next_committee_proof_valid(
    attested_header: &Header,
    next_committee: &mut SyncCommittee,
    next_committee_branch: &[Bytes32],
) -> bool {
    is_proof_valid(
        attested_header,
        next_committee,
        next_committee_branch,
        5,
        23,
    )
}
fn get_participating_keys(
    committee: &SyncCommittee,
    bitfield: &Bitvector<512>,
) -> Result<Vec<PublicKey>, ConsensusError> {
    let mut pks: Vec<PublicKey> = Vec::new();
    bitfield.iter().enumerate().for_each(|(i, bit)| {
        if bit == true {
            let pk = &committee.pubkeys[i];
            let pk = PublicKey::from_bytes_unchecked(pk).unwrap();
            pks.push(pk);
        }
    });

    Ok(pks)
}

fn verify_sync_committee_signture(
    pks: &[PublicKey],
    attested_header: &Header,
    signature: &SignatureBytes,
    signature_slot: u64,
    config: &Config,
) -> bool {
    let res: Result<bool, eyre::Report> = (move || {
        let pks: Vec<&PublicKey> = pks.iter().collect();
        let header_root = Bytes32::try_from(attested_header.clone().hash_tree_root()?.as_ref())?;
        let signing_root = compute_committee_sign_root(header_root, signature_slot, config)?;

        Ok(is_aggregate_valid(signature, signing_root.as_ref(), &pks))
    })();

    if let Ok(is_valid) = res {
        is_valid
    } else {
        false
    }
}

fn compute_committee_sign_root(
    header: Bytes32,
    slot: u64,
    config: &Config,
) -> Result<Node, ConsensusError> {
    let genesis_root = config.chain.genesis_root.to_vec().try_into().unwrap();

    let domain_type = &hex::decode("07000000").unwrap()[..];
    let fork_version = Vector::try_from(config.fork_version(slot))
        .map_err(|(_, err)| err)
        .unwrap();
    let domain = compute_domain(domain_type, fork_version, genesis_root)?;
    compute_signing_root(header, domain)
}

pub fn compute_domain(
    domain_type: &[u8],
    fork_version: Vector<u8, 4>,
    genesis_root: Bytes32,
) -> Result<Bytes32, ConsensusError> {
    let fork_data_root = compute_fork_data_root(fork_version, genesis_root)?;
    let start = domain_type;
    let end = &fork_data_root.as_ref()[..28];
    let d = [start, end].concat();
    Ok(d.to_vec().try_into().unwrap())
}

pub fn is_aggregate_valid(sig_bytes: &SignatureBytes, msg: &[u8], pks: &[&PublicKey]) -> bool {
    let sig_res = AggregateSignature::from_bytes(sig_bytes);
    match sig_res {
        Ok(sig) => sig.fast_aggregate_verify(msg, pks),
        Err(_) => false,
    }
}

fn compute_fork_data_root(
    current_version: Vector<u8, 4>,
    genesis_validator_root: Bytes32,
) -> Result<Node, ConsensusError> {
    let mut fork_data = ForkData {
        current_version,
        genesis_validator_root,
    };
    Ok(fork_data.hash_tree_root().unwrap())
}

pub fn compute_signing_root(object_root: Bytes32, domain: Bytes32) -> Result<Node, ConsensusError> {
    let mut data = SigningData {
        object_root,
        domain,
    };
    Ok(data.hash_tree_root().unwrap())
}
