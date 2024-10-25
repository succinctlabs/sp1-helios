use sp1_sdk::{NetworkProverV1, SP1ProofWithPublicValues};
use anyhow::Result;
const REQUEST_ID: &str = "proofrequest_01jb2enh32fewbwc8rej7emqky";

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let prover = NetworkProverV1::new();

    let proof: SP1ProofWithPublicValues = prover.wait_proof(REQUEST_ID, None).await.expect("Failed to fetch proof");

    proof.save("proof.bin").expect("Failed to save proof");

    Ok(())
}
