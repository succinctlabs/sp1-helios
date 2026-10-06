//! Execute recorded Gloas updates, including a fork transition and real source storage proofs.

use alloy::sol_types::SolType;
use alloy_primitives::{B256, U256};
use sp1_helios_primitives::types::{ExecutionHeaderProofOutputs, ProofInputs, ProofOutputs};
use sp1_sdk::{Elf, Prover, ProverClient, SP1Stdin};

const ELFS: [&[u8]; 2] = [
    include_bytes!("../../elf/light_client"),
    include_bytes!("../../elf/execution_header"),
];
const CURRENT: &[u8] = include_bytes!("fixtures/gloas_current.cbor");
const TRANSITION: &[u8] = include_bytes!("fixtures/gloas_transition.cbor");
const GLOAS_SLOT: u64 = 49152;

#[tokio::test]
async fn gloas_execution_and_rejection() {
    let client = ProverClient::builder().cpu().build().await;
    for fixture in [CURRENT, TRANSITION] {
        let inputs: ProofInputs = serde_cbor::from_slice(fixture).unwrap();
        let header = inputs
            .execution_header
            .as_ref()
            .expect("Gloas witness required");
        assert!(
            !inputs.contract_storage.is_empty(),
            "storage path must execute"
        );
        assert!(!inputs.contract_storage[0].storage_slots.is_empty());
        let start = inputs.store.finalized_header.beacon().slot;
        let target = inputs.finality_update.finalized_header().beacon().slot;
        assert!(target >= GLOAS_SLOT);
        if fixture == TRANSITION {
            assert!(start < GLOAS_SLOT, "transition must start before Gloas");
            assert!(start / 8192 < target / 8192, "committee must roll over");
        }
        for (mode, elf) in ELFS.iter().enumerate() {
            let mut stdin = SP1Stdin::new();
            stdin.write_slice(fixture);
            let (values, report) = client.execute(Elf::Static(elf), stdin).await.unwrap();
            assert_eq!(report.exit_code, 0);
            let (previous, head, root, number, slots) = if mode == 0 {
                let output = ProofOutputs::abi_decode(values.as_slice()).unwrap();
                (
                    output.prevHead,
                    output.newHead,
                    output.executionStateRoot,
                    output.executionBlockNumber,
                    output.storageSlots,
                )
            } else {
                let output = ExecutionHeaderProofOutputs::abi_decode(values.as_slice()).unwrap();
                assert_eq!(output.executionBlockHash, header.hash_slow());
                assert_eq!(output.executionReceiptsRoot, header.receipts_root);
                (
                    output.prevHead,
                    output.newHead,
                    output.executionStateRoot,
                    output.executionBlockNumber,
                    output.storageSlots,
                )
            };
            assert_eq!(previous, U256::from(start));
            assert_eq!(head, U256::from(target));
            assert_eq!(root, header.state_root);
            assert_eq!(number, U256::from(header.number));
            assert_eq!(slots.len(), inputs.contract_storage[0].storage_slots.len());
            assert_eq!(slots[0].contractAddress, inputs.contract_storage[0].address);
            assert_eq!(
                slots[0].key,
                inputs.contract_storage[0].storage_slots[0].key
            );
            assert_eq!(
                slots[0].value,
                B256::from(inputs.contract_storage[0].storage_slots[0].value)
            );
        }
    }
    for mutation in 0..4 {
        let mut inputs: ProofInputs = serde_cbor::from_slice(CURRENT).unwrap();
        match mutation {
            0 => inputs.execution_header = None,
            1 => inputs.execution_header.as_mut().unwrap().state_root = B256::ZERO,
            2 => inputs.contract_storage[0].storage_slots[0].value += U256::from(1),
            3 => {
                let mut finality = serde_json::to_value(&inputs.finality_update).unwrap();
                finality["finality_branch"][0] = serde_json::to_value(B256::ZERO).unwrap();
                inputs.finality_update = serde_json::from_value(finality).unwrap();
            }
            _ => unreachable!(),
        }
        let cbor = serde_cbor::to_vec(&inputs).unwrap();
        for elf in ELFS {
            let mut stdin = SP1Stdin::new();
            stdin.write_slice(&cbor);
            match client.execute(Elf::Static(elf), stdin).await {
                Ok((values, report)) => {
                    assert_ne!(report.exit_code, 0, "mutation {mutation} succeeded");
                    assert!(
                        values.as_slice().is_empty(),
                        "failed guest committed output"
                    );
                }
                Err(error) => assert!(
                    error.to_string().contains("non-zero exit code"),
                    "unexpected executor failure for mutation {mutation}: {error}"
                ),
            }
        }
    }
}
