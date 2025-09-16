use anyhow::Result;
use sp1_sdk::{HashableKey, Prover, ProverClient};

const STORAGE_ELF: &[u8] = include_bytes!("../../elf/storage");
const LIGHT_CLIENT_ELF: &[u8] = include_bytes!("../../elf/light_client");

fn main() -> Result<()> {
    let client = ProverClient::builder().cpu().build();

    let (_pk, vk) = client.setup(STORAGE_ELF);
    println!("SP1 Helios Storage Verifying Key: {:?}", vk.bytes32());

    let (_pk, vk) = client.setup(LIGHT_CLIENT_ELF);
    println!("SP1 Helios Light Client Verifying Key: {:?}", vk.bytes32());
    Ok(())
}
