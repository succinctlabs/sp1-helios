use std::time::SystemTime;

use ssz_rs::prelude::*;

use common::config::types::Forks;
use common::consensus::types::{LightClientStore, Update};
pub use ssz_rs::prelude::{Bitvector, Vector};

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct ProofInputs {
    pub update: Update,
    pub now: SystemTime,
    pub genesis_time: u64,
    pub store: LightClientStore,
    pub genesis_root: Vec<u8>,
    pub forks: Forks,
}
