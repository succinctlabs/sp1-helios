/// Adapted from reth: https://github.com/paradigmxyz/reth/blob/v1.0.1/crates/primitives/src/receipt.rs
use alloy::{
  consensus::{ReceiptEnvelope, TxType},
  core::rlp::{bytes::BufMut, length_of_length, Encodable, Header},
  primitives::{Bytes, Log},
  rpc::types::{Log as RpcLog, TransactionReceipt},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceiptWithBloomEncoder<'a> {
  // bloom: &'a Bloom,
  receipt: &'a TransactionReceipt<ReceiptEnvelope<RpcLog>>,
}

impl<'a> ReceiptWithBloomEncoder<'a> {

  /// Create new [`ReceiptWithBloom`]
  pub const fn new(receipt: &'a TransactionReceipt<ReceiptEnvelope<RpcLog>>) -> Self {
    Self { receipt }
  }

  /// Returns the enveloped encoded receipt.
  ///
  /// See also [`ReceiptWithBloom::encode_enveloped`]
  pub fn envelope_encoded(&self) -> Bytes {
    let mut buf = Vec::new();
    self.encode_enveloped(&mut buf);
    buf.into()
  }

  /// Encodes the receipt into its "raw" format.
  /// This format is also referred to as "binary" encoding.
  ///
  /// For legacy receipts, it encodes the RLP of the receipt into the buffer:
  /// `rlp([status, cumulativeGasUsed, logsBloom, logs])` as per EIP-2718.
  /// For EIP-2718 typed transactions, it encodes the type of the transaction followed by the rlp
  /// of the receipt:
  /// - EIP-1559, 2930 and 4844 transactions: `tx-type || rlp([status, cumulativeGasUsed,
  ///   logsBloom, logs])`
  pub fn encode_enveloped(&self, out: &mut dyn BufMut) {
      self.encode_inner(out, false)
  }

  /// Encode receipt with or without the header data.
  pub fn encode_inner(&self, out: &mut dyn BufMut, with_header: bool) {
      self._encode_inner(out, with_header)
  }

  fn raw_receipt(&self) -> &ReceiptEnvelope<RpcLog> {
    &self.receipt.inner
  }

  fn raw_tx_type(&self) -> TxType {
    self.receipt.transaction_type()
  }

  fn raw_logs(&self) -> Vec<&Log> {
    self.raw_receipt().logs().iter().map(|log| &log.inner).collect()
  }

  /// Returns the rlp header for the receipt payload.
  fn receipt_rlp_header(&self) -> Header {
      let mut rlp_head = Header { list: true, payload_length: 0 };

      // rlp_head.payload_length += self.receipt.success.length();
      rlp_head.payload_length += self.raw_receipt().cumulative_gas_used().length();
      // rlp_head.payload_length += self.bloom.length();
      rlp_head.payload_length += self.raw_receipt().logs_bloom().length();
      rlp_head.payload_length += self.raw_logs().length();

      // #[cfg(feature = "optimism")]
      // if self.raw_tx_type() == TxType::Deposit {
      //     if let Some(deposit_nonce) = self.receipt.deposit_nonce {
      //         rlp_head.payload_length += deposit_nonce.length();
      //     }
      //     if let Some(deposit_receipt_version) = self.receipt.deposit_receipt_version {
      //         rlp_head.payload_length += deposit_receipt_version.length();
      //     }
      // }

      rlp_head
  }

  /// Encodes the receipt data.
  fn encode_fields(&self, out: &mut dyn BufMut) {
      self.receipt_rlp_header().encode(out);

      // self.receipt.success.encode(out);
      self.raw_receipt().status().encode(out);
      self.raw_receipt().cumulative_gas_used().encode(out);
      self.raw_receipt().logs_bloom().encode(out);
      self.raw_logs().encode(out);

      // #[cfg(feature = "optimism")]
      // if self.raw_tx_type() == TxType::Deposit {
      //     if let Some(deposit_nonce) = self.receipt.deposit_nonce {
      //         deposit_nonce.encode(out)
      //     }
      //     if let Some(deposit_receipt_version) = self.receipt.deposit_receipt_version {
      //         deposit_receipt_version.encode(out)
      //     }
      // }
  }

  /// Encode receipt with or without the header data.
  fn _encode_inner(&self, out: &mut dyn BufMut, with_header: bool) {
    
      if matches!(self.raw_tx_type(), TxType::Legacy) {
          self.encode_fields(out);
          return
      }

      let mut payload = Vec::new();
      self.encode_fields(&mut payload);

      if with_header {
          let payload_length = payload.len() + 1;
          let header = Header { list: false, payload_length };
          header.encode(out);
      }

      match self.raw_tx_type() {
          TxType::Legacy => unreachable!("legacy already handled"),

          TxType::Eip2930 => {
              out.put_u8(0x01);
          }
          TxType::Eip1559 => {
              out.put_u8(0x02);
          }
          TxType::Eip4844 => {
              out.put_u8(0x03);
          }
          // #[cfg(feature = "optimism")]
          // TxType::Deposit => {
          //     out.put_u8(0x7E);
          // }
      }
      out.put_slice(payload.as_ref());
  }

  /// Returns the length of the receipt data.
  fn receipt_length(&self) -> usize {
      let rlp_head = self.receipt_rlp_header();
      length_of_length(rlp_head.payload_length) + rlp_head.payload_length
  }
}

impl<'a> Encodable for ReceiptWithBloomEncoder<'a> {

  fn encode(&self, out: &mut dyn BufMut) {
      self.encode_inner(out, true)
  }

  fn length(&self) -> usize {
      let mut payload_len = self.receipt_length();
      // account for eip-2718 type prefix and set the list
      if !matches!(self.raw_tx_type(), TxType::Legacy) {
          payload_len += 1;
          // we include a string header for typed receipts, so include the length here
          payload_len += length_of_length(payload_len);
      }

      payload_len
  }
}
