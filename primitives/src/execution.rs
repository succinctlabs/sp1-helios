//! Authenticate execution fields against a verified light-client header.

use alloy_consensus::Header;
use alloy_primitives::B256;
use anyhow::{ensure, Context, Result};
use helios_consensus_core::types::LightClientHeader;

/// Execution fields authenticated by the finalized consensus header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecutionFields {
    /// The execution state trie root.
    pub state_root: B256,
    /// The execution block number.
    pub block_number: u64,
    /// The execution block hash.
    pub block_hash: B256,
    /// The execution receipts trie root.
    pub receipts_root: B256,
}

/// Extract execution fields after consensus verification.
///
/// Gloas authenticates a block hash rather than embedding the execution payload header.
/// Its witness must hash to that commitment before any field is consumed.
pub fn verified_execution_fields(
    finalized_header: &LightClientHeader,
    execution_header: Option<&Header>,
) -> Result<ExecutionFields> {
    if let LightClientHeader::Gloas(header) = finalized_header {
        let execution = execution_header.context("Gloas execution header witness is missing")?;
        let block_hash = execution.hash_slow();
        ensure!(
            block_hash == header.execution_block_hash,
            "Execution header hash mismatch: expected {}, got {}",
            header.execution_block_hash,
            block_hash
        );
        return Ok(ExecutionFields {
            state_root: execution.state_root,
            block_number: execution.number,
            block_hash,
            receipts_root: execution.receipts_root,
        });
    }

    let execution = finalized_header
        .execution()
        .map_err(|_| anyhow::anyhow!("Finalized header has no execution commitment"))?;
    Ok(ExecutionFields {
        state_root: *execution.state_root(),
        block_number: *execution.block_number(),
        block_hash: *execution.block_hash(),
        receipts_root: *execution.receipts_root(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use helios_consensus_core::types::{
        ExecutionPayloadHeader, ExecutionPayloadHeaderElectra, LightClientHeaderElectra,
        LightClientHeaderGloas,
    };

    #[test]
    fn gloas_requires_matching_header_and_authenticates_all_fields() {
        let execution = Header {
            state_root: B256::repeat_byte(1),
            receipts_root: B256::repeat_byte(2),
            number: 42,
            block_access_list_hash: Some(B256::repeat_byte(3)),
            slot_number: Some(96),
            ..Default::default()
        };
        let header = LightClientHeader::Gloas(LightClientHeaderGloas {
            execution_block_hash: execution.hash_slow(),
            ..Default::default()
        });
        assert!(header.execution().is_err());
        assert!(verified_execution_fields(&header, None).is_err());
        let fields = verified_execution_fields(&header, Some(&execution)).unwrap();
        assert_eq!(fields.state_root, execution.state_root);
        assert_eq!(fields.receipts_root, execution.receipts_root);
        assert_eq!(fields.block_number, execution.number);

        for changed in [
            Header {
                state_root: B256::ZERO,
                ..execution.clone()
            },
            Header {
                receipts_root: B256::ZERO,
                ..execution.clone()
            },
            Header {
                number: 43,
                ..execution.clone()
            },
            Header {
                block_access_list_hash: None,
                ..execution.clone()
            },
            Header {
                slot_number: None,
                ..execution.clone()
            },
        ] {
            assert!(verified_execution_fields(&header, Some(&changed)).is_err());
        }
    }

    #[test]
    fn legacy_header_keeps_authenticated_embedded_fields() {
        let execution = ExecutionPayloadHeaderElectra {
            state_root: B256::repeat_byte(1),
            receipts_root: B256::repeat_byte(2),
            block_number: 42,
            block_hash: B256::repeat_byte(3),
            ..Default::default()
        };
        let header = LightClientHeader::Electra(LightClientHeaderElectra {
            execution: ExecutionPayloadHeader::Electra(execution.clone()),
            ..Default::default()
        });
        let fields = verified_execution_fields(&header, None).unwrap();
        assert_eq!(fields.state_root, execution.state_root);
        assert_eq!(fields.block_hash, execution.block_hash);
        assert_eq!(fields.block_number, execution.block_number);
    }
}
