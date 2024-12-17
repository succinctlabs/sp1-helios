use alloy::rpc::types::TransactionReceipt;
use alloy_primitives::{bytes::BufMut, Bloom};

// pub trait TransactionReceiptRlp {
//   fn rlp_encoded_fields_length(&self, bloom: &Bloom) -> usize;
//   fn rlp_encode_fields(&self, bloom: &Bloom, out: &mut dyn BufMut);
// }

// impl TransactionReceiptRlp for TransactionReceipt {
//   /// Returns length of RLP-encoded receipt fields with the given [`Bloom`] without an RLP header.
//   fn rlp_encoded_fields_length(&self, bloom: &Bloom) -> usize {
//       self.inner.status().length() +
//           self.inner.cumulative_gas_used().length() +
//           self.inner.logs_bloom().length() +
//           self.inner.logs().length()
//   }

//   /// RLP-encodes receipt fields with the given [`Bloom`] without an RLP header.
//   fn rlp_encode_fields(&self, bloom: &Bloom, out: &mut dyn BufMut) {
//       // self.success.encode(out);
//       // self.cumulative_gas_used.encode(out);
//       // bloom.encode(out);
//       // self.logs.encode(out);
//   }
// }
