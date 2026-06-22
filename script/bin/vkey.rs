use anyhow::Result;
use sp1_sdk::{HashableKey, Prover, ProverClient, ProvingKey};

const STORAGE_ELF: &[u8] = include_bytes!("../../elf/storage");
const LIGHT_CLIENT_ELF: &[u8] = include_bytes!("../../elf/light_client");
const EXECUTION_HEADER_ELF: &[u8] = include_bytes!("../../elf/execution_header");

#[tokio::main]
async fn main() -> Result<()> {
    let client = ProverClient::builder().cpu().build().await;

    let pk = client.setup(STORAGE_ELF.into()).await?;
    println!(
        "SP1 Helios Storage Verifying Key: {:?}",
        pk.verifying_key().bytes32()
    );

    let pk = client.setup(LIGHT_CLIENT_ELF.into()).await?;
    println!(
        "SP1 Helios Light Client Verifying Key: {:?}",
        pk.verifying_key().bytes32()
    );

    let pk = client.setup(EXECUTION_HEADER_ELF.into()).await?;
    println!(
        "SP1 Helios Execution Header Verifying Key: {:?}",
        pk.verifying_key().bytes32()
    );
    Ok(())
}
