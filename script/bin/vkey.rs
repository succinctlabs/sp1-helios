use alloy_primitives::B256;
use anyhow::Result;
use sp1_sdk::{HashableKey, ProverClient};

const HELIOS_ELF: &[u8] = include_bytes!("../../elf/sp1-helios-docker");

fn main() -> Result<()> {
    let client = ProverClient::new();
    let (_pk, vk) = client.setup(HELIOS_ELF);
    println!(
        "SP1 Helios Verifying Key: {:?}",
        B256::from(vk.hash_bytes())
    );
    Ok(())
}
