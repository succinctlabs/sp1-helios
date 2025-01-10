use anyhow::Result;
use sp1_sdk::{HashableKey, Prover, ProverClient};

const HELIOS_ELF: &[u8] = include_bytes!("../../elf/sp1-helios-elf");

fn main() -> Result<()> {
    let client = ProverClient::builder().cpu().build();
    let (_pk, vk) = client.setup(HELIOS_ELF);
    println!("SP1 Helios Verifying Key: {:?}", vk.bytes32());
    Ok(())
}
