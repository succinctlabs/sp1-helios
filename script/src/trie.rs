/// Adapted from reth: https://github.com/paradigmxyz/reth/blob/v1.0.1/crates/trie/common/src/root.rs

use alloy::{
  core::rlp::{Encodable, encode_fixed_size},
  primitives::{Address, Bloom, B256, Log, U256},
};
use alloy_trie::HashBuilder;
use nybbles::Nibbles;

/// Adjust the index of an item for rlp encoding.
pub const fn adjust_index_for_rlp(i: usize, len: usize) -> usize {
  if i > 0x7f {
      i
  } else if i == 0x7f || i + 1 == len {
      0
  } else {
      i + 1
  }
}

/// Compute a trie root of the collection of rlp encodable items.
pub fn ordered_trie_root<T: Encodable>(items: &[T]) -> B256 {
  ordered_trie_root_with_encoder(items, |item, buf| item.encode(buf))
}

/// Compute a trie root of the collection of items with a custom encoder.
pub fn ordered_trie_root_with_encoder<T, F>(items: &[T], mut encode: F) -> B256
where
  F: FnMut(&T, &mut Vec<u8>),
{
  let mut value_buffer = Vec::new();

  let mut hb = HashBuilder::default();
  let items_len = items.len();
  for i in 0..items_len {
      let index = adjust_index_for_rlp(i, items_len);

      let index_buffer = encode_fixed_size(&index);

      value_buffer.clear();
      encode(&items[index], &mut value_buffer);

      hb.add_leaf(Nibbles::unpack(&index_buffer), &value_buffer);
  }

  hb.root()
}
