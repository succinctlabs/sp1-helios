use alloy_primitives::B256;
/// Continuously generate proofs & keep light client updated with chain
use anyhow::Result;
use ethers::{
    prelude::*,
    providers::{Http, Middleware, Provider},
    signers::{LocalWallet, Signer},
};
use helios::consensus::rpc::ConsensusRpc;
use helios::consensus::{rpc::nimbus_rpc::NimbusRpc, Inner};
use helios_script::*;
use log::{error, info};
use sp1_helios_primitives::types::ProofInputs;
use sp1_sdk::{ProverClient, SP1ProofWithPublicValues, SP1ProvingKey, SP1Stdin};

use std::env;
use std::sync::Arc;

const ELF: &[u8] = include_bytes!("../../program/elf/riscv32im-succinct-zkvm-elf");

abigen!(
    SP1LightClient,
    r#"[
        function update(bytes calldata proof, bytes calldata publicValues) external
        function head() external view returns (uint256)
        function getSyncCommitteePeriod(uint256 slot) external view returns (uint256)
        function syncCommittees(uint256 period) external view returns (bytes32)
    ]"#
);

struct SP1LightClientOperator {
    client: ProverClient,
    pk: SP1ProvingKey,
    provider: Provider<Http>,
    contract: SP1LightClient<SignerMiddleware<Provider<Http>, LocalWallet>>,
    chain_id: u64,
    relayer_address: Address,
}

impl SP1LightClientOperator {
    pub async fn new() -> Self {
        dotenv::dotenv().ok();

        let client = ProverClient::new();
        let (pk, _) = client.setup(ELF);
        let chain_id: u64 = env::var("CHAIN_ID")
            .expect("CHAIN_ID not set")
            .parse()
            .unwrap();
        let rpc_url = env::var("RPC_URL").expect("RPC_URL not set");

        let private_key = env::var("PRIVATE_KEY").expect("PRIVATE_KEY not set");
        let contract_address: Address = env::var("CONTRACT_ADDRESS")
            .expect("CONTRACT_ADDRESS not set")
            .parse()
            .unwrap();

        let provider = Provider::<Http>::try_from(rpc_url).unwrap();
        let wallet: LocalWallet = private_key.parse().expect("Failed to parse private key");
        let relayer_address = wallet.address();
        let signer = SignerMiddleware::new(provider.clone(), wallet);

        let contract = SP1LightClient::new(contract_address, Arc::new(signer));

        Self {
            client,
            pk,
            provider,
            contract,
            chain_id,
            relayer_address,
        }
    }

    async fn request_update(
        &self,
        client: Inner<NimbusRpc>,
    ) -> Result<Option<SP1ProofWithPublicValues>> {
        // Fetch required values.
        let head: u64 = self.contract.head().call().await?.as_u64();
        let period: u64 = self
            .contract
            .get_sync_committee_period(head.into())
            .call()
            .await?
            .as_u64();
        let contract_current_sync_committee =
            self.contract.sync_committees(period.into()).call().await?;
        let contract_next_sync_committee = self
            .contract
            .sync_committees((period + 1).into())
            .call()
            .await?;

        let mut stdin = SP1Stdin::new();

        // Setup client.
        let updates = get_updates(&client).await;
        let (client, updates) = sync_client(
            client,
            updates,
            head,
            B256::from(contract_current_sync_committee),
            B256::from(contract_next_sync_committee),
        )
        .await;

        let finality_update = client.rpc.get_finality_update().await.unwrap();
        let latest_block = finality_update.finalized_header.slot;

        if latest_block.as_u64() <= head {
            info!("Contract is up to date. Nothing to update.");
            return Ok(None);
        }

        let execution_state_proof = get_execution_state_root_proof(latest_block.into())
            .await
            .unwrap();

        let expected_current_slot = client.expected_current_slot();
        let inputs = ProofInputs {
            updates,
            finality_update,
            expected_current_slot,
            store: client.store,
            genesis_root: client.config.chain.genesis_root.clone().try_into().unwrap(),
            forks: client.config.forks.clone(),
            execution_state_proof,
        };

        let encoded_proof_inputs = serde_cbor::to_vec(&inputs)?;
        stdin.write_slice(&encoded_proof_inputs);

        // Generate proof.
        let proof = self.client.prove(&self.pk, stdin).plonk().run().unwrap();

        info!("New head: {:?}", latest_block.as_u64());
        Ok(Some(proof))
    }

    async fn relay_update(&self, proof: SP1ProofWithPublicValues) -> Result<()> {
        let proof_as_bytes = if env::var("SP1_PROVER").unwrap().to_lowercase() == "mock" {
            vec![]
        } else {
            proof.bytes()
        };
        let public_values_bytes = proof.public_values.to_vec();

        let gas_limit = get_gas_limit(self.chain_id);
        let max_fee_per_gas = get_fee_cap(self.chain_id, &self.provider).await;

        let nonce = self
            .provider
            .get_transaction_count(self.relayer_address, None)
            .await?;

        const NUM_CONFIRMATIONS: usize = 3;

        let tx = self
            .contract
            .update(proof_as_bytes.into(), public_values_bytes.into())
            .gas(gas_limit)
            .gas_price(max_fee_per_gas)
            .nonce(nonce);

        let pending_tx = tx.send().await?;
        let receipt = pending_tx
            .confirmations(NUM_CONFIRMATIONS)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Transaction failed to confirm"))?;

        if receipt.status != Some(1.into()) {
            error!("Transaction reverted!");
        }

        info!("Transaction hash: {:?}", receipt.transaction_hash);

        Ok(())
    }

    async fn run(&mut self, loop_delay_mins: u64) -> Result<()> {
        info!("Starting SP1 Telepathy operator");

        loop {
            let contract =
                SP1LightClient::new(self.contract.address(), Arc::new(self.provider.clone()));

            // Get the current slot from the contract
            let slot = contract.head().call().await?.as_u64();

            // Fetch the checkpoint at that slot
            let checkpoint = get_checkpoint(slot).await;

            // Get the client from the checkpoint
            let client = get_client(checkpoint.as_bytes().to_vec()).await;

            // Request an update
            match self.request_update(client).await {
                Ok(Some(proof)) => {
                    self.relay_update(proof).await?;
                }
                Ok(None) => {
                    // Contract is up to date. Nothing to update.
                }
                Err(e) => {
                    error!("Header range request failed: {}", e);
                }
            };

            info!("Sleeping for {:?} minutes", loop_delay_mins);
            tokio::time::sleep(tokio::time::Duration::from_secs(60 * loop_delay_mins)).await;
        }
    }
}

fn get_gas_limit(chain_id: u64) -> U256 {
    match chain_id {
        42161 | 421614 => U256::from(25_000_000),
        _ => U256::from(1_500_000),
    }
}

async fn get_fee_cap(chain_id: u64, provider: &Provider<Http>) -> U256 {
    let multiplier =
        if chain_id == 17000 || chain_id == 421614 || chain_id == 11155111 || chain_id == 84532 {
            100
        } else {
            20
        };

    let gas_price = provider.get_gas_price().await.unwrap();

    gas_price * (100 + multiplier) / 100
}

#[tokio::main]
async fn main() {
    env::set_var("RUST_LOG", "info");
    dotenv::dotenv().ok();
    env_logger::init();

    let loop_delay_mins = match env::var("LOOP_DELAY_MINS") {
        Ok(value) if value.is_empty() => 5, // Use default if empty
        Ok(value) => value.parse().expect("Invalid LOOP_DELAY_MINS"),
        Err(_) => 5, // Use default if not set
    };

    let mut operator = SP1LightClientOperator::new().await;
    loop {
        if let Err(e) = operator.run(loop_delay_mins).await {
            error!("Error running operator: {}", e);
        }
    }
}
