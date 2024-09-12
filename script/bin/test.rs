use anyhow::Result;
use sp1_helios_primitives::types::ProofInputs;
use sp1_sdk::{utils::setup_logger, ProverClient, SP1Stdin};
use std::fs::File;
use std::io::Read;

const ELF: &[u8] = include_bytes!("../../elf/riscv32im-succinct-zkvm-elf");

fn main() -> Result<()> {
    dotenv::dotenv().ok();
    setup_logger();
    // Read CBOR data from file
    let mut file = File::open("examples/input.cbor")?;
    let mut cbor_data = Vec::new();
    file.read_to_end(&mut cbor_data)?;

    // Deserialize the data into the ProofInputs struct
    let inputs: ProofInputs = serde_cbor::from_slice(&cbor_data)?;

    // Write the inputs to the VM
    let mut stdin = SP1Stdin::new();
    stdin.write_slice(&serde_cbor::to_vec(&inputs)?);

    let prover_client = ProverClient::new();
    let (_, report) = prover_client.execute(ELF, stdin).run()?;
    println!("Execution Report: {:?}", report);

    Ok(())
}
