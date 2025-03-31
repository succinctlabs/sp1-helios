use alloy::{
    eips::BlockId,
    providers::{Provider, ProviderBuilder},
};
use alloy_primitives::{address, b256, U256};
use alloy_trie::TrieAccount;
use anyhow::Result;
use clap::{command, Parser};
use helios_ethereum::rpc::ConsensusRpc;
use sp1_helios_primitives::types::{ContractStorage, ProofInputs, StorageSlot};
use sp1_helios_script::{get_checkpoint, get_client, get_latest_checkpoint, get_updates};
use sp1_sdk::{utils::setup_logger, ProverClient, SP1Stdin};

const ELF: &[u8] = include_bytes!("../../elf/sp1-helios-elf");
#[derive(Parser, Debug, Clone)]
#[command(about = "Get the genesis parameters from a block.")]
pub struct GenesisArgs {
    #[arg(long)]
    pub slot: Option<u64>,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    setup_logger();
    let args = GenesisArgs::parse();

    // Get the current slot from the contract or fetch the latest checkpoint
    let checkpoint = if let Some(slot) = args.slot {
        get_checkpoint(slot).await
    } else {
        get_latest_checkpoint().await
    };

    // Setup client.
    let helios_client = get_client(checkpoint).await;
    let sync_committee_updates = get_updates(&helios_client).await;
    let finality_update = helios_client.rpc.get_finality_update().await.unwrap();

    let expected_current_slot = helios_client.expected_current_slot();

    // Get the block number for the current slot
    let block_number = helios_client
        .store
        .finalized_header
        .execution()
        .unwrap()
        .block_number();

    // Expected values for the proof (mainnet Across SpokePool->crossDomainAdmin()).
    let contract_address = address!("0x5c7BCd6E7De5423a257D81B442095A1a6ced35C5");
    let storage_slot = b256!("0000000000000000000000000000000000000000000000000000000000000869");
    let expected_value = address!("c186fA914353c44b2E33eBE05f21846F1048bEda");

    // Setup execution RPC client
    let execution_rpc = std::env::var("SOURCE_EXECUTION_RPC_URL").unwrap();
    let provider = ProviderBuilder::new().on_http(execution_rpc.parse()?);

    // Get the proof using eth_getProof
    let proof = provider
        .get_proof(contract_address, vec![storage_slot])
        .block_id(BlockId::number(*block_number))
        .await?;

    let inputs = ProofInputs {
        sync_committee_updates,
        finality_update,
        expected_current_slot,
        store: helios_client.store,
        genesis_root: helios_client.config.chain.genesis_root,
        forks: helios_client.config.forks.clone(),
        contract_storage_slots: ContractStorage {
            address: contract_address,
            expected_value: TrieAccount {
                nonce: proof.nonce,
                balance: proof.balance,
                storage_root: proof.storage_hash,
                code_hash: proof.code_hash,
            },
            mpt_proof: proof.account_proof,
            storage_slots: vec![StorageSlot {
                key: storage_slot,
                expected_value: U256::from_be_slice(expected_value.as_slice()),
                mpt_proof: proof.storage_proof[0].proof.clone(),
            }],
        },
    };

    // Write the inputs to the VM
    let mut stdin = SP1Stdin::new();
    stdin.write_slice(&serde_cbor::to_vec(&inputs)?);

    // Configure a ProverClient for testing
    let prover_client = ProverClient::builder().cpu().build();
    let (_, report) = prover_client.execute(ELF, &stdin).run()?;
    println!("Execution Report: {:?}", report);

    Ok(())
}
