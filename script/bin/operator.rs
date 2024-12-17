/// Continuously generate proofs & keep light client updated with chain
use alloy::{
    consensus::Receipt, eips::BlockNumberOrTag, network::{Ethereum, EthereumWallet}, primitives::Address, providers::{
        fillers::{ChainIdFiller, FillProvider, GasFiller, JoinFill, NonceFiller, WalletFiller},
        Identity, Provider, ProviderBuilder, RootProvider,
    }, rpc::{types::TransactionReceipt}, signers::{k256::pkcs8::der::Encode, local::PrivateKeySigner}, sol, transports::http::{Client, Http}
};
// use alloy_merkle_tree::tree::MerkleTree;
use alloy_primitives::{Bloom, B256, U256};
use anyhow::Result;
use helios_consensus_core::{consensus_spec::MainnetConsensusSpec, types::ExecutionPayload};
use helios_ethereum::consensus::Inner;
use helios_ethereum::rpc::http_rpc::HttpRpc;
use helios_ethereum::rpc::ConsensusRpc;
use log::{error, info};
use sp1_helios_primitives::types::ProofInputs;
use sp1_helios_script::*;
use sp1_sdk::{ProverClient, SP1ProofWithPublicValues, SP1ProvingKey, SP1Stdin};
use ssz_rs::prelude::*;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tree_hash::TreeHash;
// use alloy_rlp::{bytes::BufMut, BytesMut};

const ELF: &[u8] = include_bytes!("../../elf/riscv32im-succinct-zkvm-elf");

/// Alias the fill provider for the Ethereum network. Retrieved from the instantiation of the
/// ProviderBuilder. Recommended method for passing around a ProviderBuilder.
type EthereumFillProvider = FillProvider<
    JoinFill<
        JoinFill<JoinFill<JoinFill<Identity, GasFiller>, NonceFiller>, ChainIdFiller>,
        WalletFiller<EthereumWallet>,
    >,
    RootProvider<Http<Client>>,
    Http<Client>,
    Ethereum,
>;

struct SP1LightClientOperator {
    client: ProverClient,
    pk: SP1ProvingKey,
    wallet_filler: Arc<EthereumFillProvider>,
    contract_address: Address,
    relayer_address: Address,
    chain_id: u64,
}

sol! {
    #[allow(missing_docs)]
    #[sol(rpc)]
    contract SP1LightClient {
        bytes32 public immutable GENESIS_VALIDATORS_ROOT;
        uint256 public immutable GENESIS_TIME;
        uint256 public immutable SECONDS_PER_SLOT;
        uint256 public immutable SLOTS_PER_PERIOD;
        uint32 public immutable SOURCE_CHAIN_ID;
        uint256 public head;
        mapping(uint256 => bytes32) public syncCommittees;
        mapping(uint256 => bytes32) public executionStateRoots;
        mapping(uint256 => bytes32) public headers;
        bytes32 public heliosProgramVkey;
        address public verifier;

        struct ProofOutputs {
            bytes32 executionStateRoot;
            bytes32 newHeader;
            bytes32 nextSyncCommitteeHash;
            uint256 newHead;
            bytes32 prevHeader;
            uint256 prevHead;
            bytes32 syncCommitteeHash;
        }

        event HeadUpdate(uint256 indexed slot, bytes32 indexed root);
        event SyncCommitteeUpdate(uint256 indexed period, bytes32 indexed root);

        function update(bytes calldata proof, bytes calldata publicValues) external;
        function getSyncCommitteePeriod(uint256 slot) internal view returns (uint256);
        function getCurrentSlot() internal view returns (uint256);
        function getCurrentEpoch() internal view returns (uint256);
    }
}

impl SP1LightClientOperator {
    pub async fn new() -> Self {
        dotenv::dotenv().ok();

        let client = ProverClient::new();
        let (pk, _) = client.setup(ELF);
        let chain_id: u64 = env::var("DEST_CHAIN_ID")
            .expect("DEST_CHAIN_ID not set")
            .parse()
            .unwrap();
        let rpc_url = env::var("DEST_RPC_URL")
            .expect("DEST_RPC_URL not set")
            .parse()
            .unwrap();

        let private_key = env::var("PRIVATE_KEY").expect("PRIVATE_KEY not set");
        let contract_address: Address = env::var("CONTRACT_ADDRESS")
            .expect("CONTRACT_ADDRESS not set")
            .parse()
            .unwrap();
        let signer: PrivateKeySigner = private_key.parse().expect("Failed to parse private key");
        let relayer_address = signer.address();
        let wallet = EthereumWallet::from(signer);
        let provider = ProviderBuilder::new()
            .with_recommended_fillers()
            .wallet(wallet)
            .on_http(rpc_url);

        Self {
            client,
            pk,
            wallet_filler: Arc::new(provider),
            chain_id,
            contract_address,
            relayer_address,
        }
    }

    /// Fetch values and generate an 'update' proof for the SP1 LightClient contract.
    async fn request_update(
        &self,
        mut client: Inner<MainnetConsensusSpec, HttpRpc>,
        target_block: u64,
        // contract_address,
        // event
    ) -> Result<Option<SP1ProofWithPublicValues>> {
        // Fetch required values.
        let contract = SP1LightClient::new(self.contract_address, self.wallet_filler.clone());
        let head: u64 = contract
            .head()
            .call()
            .await
            .unwrap()
            .head
            .try_into()
            .unwrap();
        let period: u64 = contract
            .getSyncCommitteePeriod(U256::from(head))
            .call()
            .await
            .unwrap()
            ._0
            .try_into()
            .unwrap();
        let contract_next_sync_committee = contract
            .syncCommittees(U256::from(period + 1))
            .call()
            .await
            .unwrap()
            ._0;

        let mut stdin = SP1Stdin::new();

        // Setup client.
        let mut sync_committee_updates = get_updates(&client).await;
        let finality_update = client.rpc.get_finality_update().await.unwrap();


        // Check if contract is up to date
        let latest_block = finality_update.finalized_header.beacon().slot;

        if latest_block <= head {
            info!("Contract is up to date. Nothing to update.");
            return Ok(None);
        }

        // Introspect target block
        if latest_block < target_block {
            info!("Target block not reached, yet.");
            return Ok(None);
        }

        let consensus_block = client.rpc.get_block(target_block).await.unwrap();
        let execution_payload = consensus_block.body.execution_payload();

        let block = BlockNumberOrTag::from(*execution_payload.block_number());
        let receipts_root = execution_payload.receipts_root();
        let receipts = self.wallet_filler.get_block_receipts(block).await.unwrap().unwrap();

        // let mut tree = MerkleTree::new();
        for receipt in receipts {

            let r: TransactionReceipt = receipt;

            let _r = r.inner.as_receipt_with_bloom().unwrap();
            // let rr = Receipt::from(*_r);
            // rr.en

            // ordered_trie_root_with_encoder();

            // let rrr = calculate_receipt_root();

            // pub fn calculate_receipt_root<T>(receipts: &[T]) -> B256
            // where
            //     T: Encodable2718,
            // {
            //     ordered_trie_root_with_encoder(receipts, |r, buf| r.encode_2718(buf))
            // }

            // mut dyn BufMut
            // let mut out = Vec::with_capacity(10000);

            // receipt.inner.status().encode(&mut out);
            // receipt.inner.cumulative_gas_used().encode(&mut out);
            // receipt.inner.logs_bloom().encode(&mut out);
            // receipt.inner.logs().encode(&mut out);

            // let mut buf = vec![];
            // buf.put(receipt.inner.as_receipt_with_bloom().unwrap());

            // let r = RlpReceipt {
            //     status: receipt.status(),
            //     cumulative_gas_used: receipt.inner.cumulative_gas_used(),
            //     logs_bloom: receipt.inner.logs_bloom().clone(),
            //     logs: receipt.inner.logs().clone(),
            // };

            // let mut stream = RlpStream::new();
            // stream.begin_list(4);
            // stream.append(receipt.status());
            // stream.append(receipt.inner.cumulative_gas_used());
            // stream.append(receipt.inner.logs_bloom());
            // stream.append_list(receipt.inner.logs());
            // stream.out().copy_to_bytes(out.remaining_mut());

            tree.insert(r.encode());
        }




        // // Should be 2 ^ N leaves
        // let num_leaves = 16;
        // for i in 0..num_leaves {
        //     tree.insert(B256::from(U256::from(i)));
        // }
        // tree.finish();

        // for i in 0..num_leaves {
        //     let proof = tree.create_proof(&B256::from(U256::from(i))).unwrap();
        //     assert!(MerkleTree::verify_proof(&proof));
        // }

        // RLP encode receipts
        // let encoded_receipts: Vec<Vec<u8>> = receipts
        //     .iter()
        //     .map(|receipt| {
        //         let mut stream = RlpStream::new();
        //         receipt.rlp_append(&mut stream);
        //         stream.out()
        //     })
        //     .collect();

        // // Construct the Patricia Trie
        // let mut backend = MemoryBackend::<Vec<u8>, H256>::default();
        // let mut trie = Trie::new(&mut backend);

        // for (index, encoded_receipt) in encoded_receipts.iter().enumerate() {
        //     let key = index.to_be_bytes().to_vec(); // Transaction index as key
        //     trie.insert(&key, encoded_receipt).unwrap();
        // }

        // Compute the receiptsRoot
        // let receipts_root = trie.commit().unwrap();



        let parent_hash = execution_payload.parent_hash();
        let block_hash = execution_payload.block_hash();

        
        let body_root = beacon.body_root;
        // TOOD: randao reveal, eth1_data, graffiti, proposer_slashings, attester_slashings..., execution_payload


        let execution = header.execution().unwrap();
        let execution_block_hash = execution.block_hash();
        let receipt_root = execution.receipts_root();
        // TODO: Exeuction Payload: parentHash, blockHash, stateRoot, logsBloom, receiptsRoot, transactions, baseFeePerGas, gasUsed, etc...
        // Receipt root: Transaction status, Cumulative gas used, Logs (address, topics, data), Bloom filter for logs.

        // TODO: show that block hash is part of execution payload, which is part of beacon body root

        
        // Use alloy to fetch data
        // curl -X POST -H "Content-Type: application/json" \
        // --data '{"jsonrpc":"2.0","method":"eth_getBlockByNumber","params":["0x1b4", true],"id":1}' \
        // http://127.0.0.1:8545





        // Optimization:
        // Skip processing update inside program if next_sync_committee is already stored in contract.
        // We must still apply the update locally to "sync" the helios client, this is due to
        // next_sync_committee not being stored when the helios client is bootstrapped.
        if !sync_committee_updates.is_empty() {
            let next_sync_committee = B256::from_slice(
                sync_committee_updates[0]
                    .next_sync_committee
                    .tree_hash_root()
                    .as_ref(),
            );

            if contract_next_sync_committee == next_sync_committee {
                println!("Applying optimization, skipping update");
                let temp_update = sync_committee_updates.remove(0);

                client.verify_update(&temp_update).unwrap(); // Panics if not valid
                client.apply_update(&temp_update);
            }
        }

        // Create program inputs
        let expected_current_slot = client.expected_current_slot();
        let inputs = ProofInputs {
            sync_committee_updates,
            finality_update,
            expected_current_slot,
            store: client.store.clone(),
            genesis_root: client.config.chain.genesis_root,
            forks: client.config.forks.clone(),
        };
        let encoded_proof_inputs = serde_cbor::to_vec(&inputs)?;
        stdin.write_slice(&encoded_proof_inputs);

        // Generate proof.
        let proof = self.client.prove(&self.pk, stdin).groth16().run()?;

        info!("Attempting to update to new head block: {:?}", latest_block);
        Ok(Some(proof))
    }

    /// Relay an update proof to the SP1 LightClient contract.
    async fn relay_update(&self, proof: SP1ProofWithPublicValues) -> Result<()> {
        let proof_as_bytes = if env::var("SP1_PROVER").unwrap().to_lowercase() == "mock" {
            vec![]
        } else {
            proof.bytes()
        };
        let public_values_bytes = proof.public_values.to_vec();

        let contract = SP1LightClient::new(self.contract_address, self.wallet_filler.clone());

        let gas_limit = relay::get_gas_limit(self.chain_id);
        let max_fee_per_gas = relay::get_fee_cap(self.chain_id, self.wallet_filler.root()).await;

        let nonce = self
            .wallet_filler
            .get_transaction_count(self.relayer_address)
            .await?;

        // Wait for 3 required confirmations with a timeout of 60 seconds.
        const NUM_CONFIRMATIONS: u64 = 3;
        const TIMEOUT_SECONDS: u64 = 60;
        let receipt = contract
            .update(proof_as_bytes.into(), public_values_bytes.into())
            .gas_price(max_fee_per_gas)
            .gas(gas_limit)
            .nonce(nonce)
            .send()
            .await?
            .with_required_confirmations(NUM_CONFIRMATIONS)
            .with_timeout(Some(Duration::from_secs(TIMEOUT_SECONDS)))
            .get_receipt()
            .await?;

        // If status is false, it reverted.
        if !receipt.status() {
            error!("Transaction reverted!");
            return Err(anyhow::anyhow!("Transaction reverted!"));
        }

        info!(
            "Successfully updated to new head block! Tx hash: {:?}",
            receipt.transaction_hash
        );

        Ok(())
    }

    /// Start the operator.
    async fn run(&mut self, loop_delay_mins: u64) -> Result<()> {
        info!("Starting SP1 Helios operator");

        loop {
            let contract = SP1LightClient::new(self.contract_address, self.wallet_filler.clone());

            // Get the current slot from the contract
            let slot = contract
                .head()
                .call()
                .await
                .unwrap_or_else(|e| {
                    panic!("Failed to get head. Are you sure the SP1LightClient is deployed to address: {:?}? Error: {:?}", self.contract_address, e)
                })
                .head
                .try_into()
                .unwrap();

            // Fetch the checkpoint at that slot
            let checkpoint = get_checkpoint(slot).await;

            // Get the client from the checkpoint
            let client = get_client(checkpoint).await;

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

#[tokio::main]
async fn main() -> Result<()> {
    env::set_var("RUST_LOG", "info");
    dotenv::dotenv().ok();
    env_logger::init();

    let loop_delay_mins = env::var("LOOP_DELAY_MINS")
        .unwrap_or("5".to_string())
        .parse()?;

    let mut operator = SP1LightClientOperator::new().await;
    loop {
        if let Err(e) = operator.run(loop_delay_mins).await {
            error!("Error running operator: {}", e);
        }
    }
}
