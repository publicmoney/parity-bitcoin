use bitcrypto::SHA256D;
use chain::IndexedBlock;
use linked_hash_map::LinkedHashMap;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet, VecDeque};
use time;

#[derive(Debug)]
/// Storage for blocks, for which we have no parent yet.
/// Blocks from this storage are either moved to verification queue, or removed at all.
pub struct OrphanBlocksPool {
	/// Blocks from requested_hashes, but received out-of-order.
	orphaned_blocks: HashMap<SHA256D, HashMap<SHA256D, IndexedBlock>>,
	/// Blocks that we have received without requesting with receiving time.
	unknown_blocks: LinkedHashMap<SHA256D, f64>,
}

impl OrphanBlocksPool {
	/// Create new pool
	pub fn new() -> Self {
		OrphanBlocksPool {
			orphaned_blocks: HashMap::new(),
			unknown_blocks: LinkedHashMap::new(),
		}
	}

	/// Get total number of blocks in pool
	pub fn len(&self) -> usize {
		self.orphaned_blocks.len()
	}

	/// Check if block with given hash is stored as unknown in this pool
	pub fn contains_unknown_block(&self, hash: &SHA256D) -> bool {
		self.unknown_blocks.contains_key(hash)
	}

	/// Get unknown blocks in the insertion order
	pub fn unknown_blocks(&self) -> &LinkedHashMap<SHA256D, f64> {
		&self.unknown_blocks
	}

	/// Insert orphaned block, for which we have already requested its parent block
	pub fn insert_orphaned_block(&mut self, block: IndexedBlock) {
		self.orphaned_blocks
			.entry(block.header.raw.previous_header_hash.clone())
			.or_insert_with(HashMap::new)
			.insert(block.header.hash.clone(), block);
	}

	/// Insert unknown block, for which we know nothing about its parent block
	pub fn insert_unknown_block(&mut self, block: IndexedBlock) {
		let previous_value = self.unknown_blocks.insert(block.header.hash.clone(), time::precise_time_s());
		assert_eq!(previous_value, None);

		self.insert_orphaned_block(block);
	}

	/// Remove all blocks, which are not-unknown
	pub fn remove_known_blocks(&mut self) -> Vec<SHA256D> {
		let orphans_to_remove: HashSet<_> = self
			.orphaned_blocks
			.values()
			.flat_map(|v| v.iter().map(|e| e.0.clone()))
			.filter(|h| !self.unknown_blocks.contains_key(h))
			.collect();
		self.remove_blocks(&orphans_to_remove);
		orphans_to_remove.into_iter().collect()
	}

	/// Remove all blocks, depending on this parent
	pub fn remove_blocks_for_parent(&mut self, hash: &SHA256D) -> VecDeque<IndexedBlock> {
		let mut queue: VecDeque<SHA256D> = VecDeque::new();
		queue.push_back(hash.clone());

		let mut removed: VecDeque<IndexedBlock> = VecDeque::new();
		while let Some(parent_hash) = queue.pop_front() {
			if let Entry::Occupied(entry) = self.orphaned_blocks.entry(parent_hash) {
				let (_, orphaned) = entry.remove_entry();
				for orphaned_hash in orphaned.keys() {
					self.unknown_blocks.remove(orphaned_hash);
				}
				queue.extend(orphaned.keys().cloned());
				removed.extend(orphaned.into_iter().map(|(_, b)| b));
			}
		}
		removed
	}

	/// Remove blocks with given hashes + all dependent blocks
	pub fn remove_blocks(&mut self, hashes: &HashSet<SHA256D>) -> Vec<SHA256D> {
		let mut removed: Vec<SHA256D> = Vec::new();

		self.orphaned_blocks.retain(|_, orphans| {
			for hash in hashes {
				orphans.remove(hash).map(|_| removed.push(*hash));
			}
			!orphans.is_empty()
		});

		for block in &removed {
			self.unknown_blocks.remove(block);
		}
		// also delete all children
		for hash in hashes.iter() {
			removed.extend(self.remove_blocks_for_parent(hash).iter().map(|block| block.hash()));
		}

		removed
	}
}

#[cfg(test)]
mod tests {
	extern crate test_data;

	use super::OrphanBlocksPool;
	use bitcrypto::SHA256D;
	use std::collections::HashSet;

	#[test]
	fn orphan_block_pool_empty_on_start() {
		let pool = OrphanBlocksPool::new();
		assert_eq!(pool.len(), 0);
	}

	#[test]
	fn orphan_block_pool_insert_orphan_block() {
		let mut pool = OrphanBlocksPool::new();
		let b1 = test_data::block_h1();
		let b1_hash = b1.hash();

		pool.insert_orphaned_block(b1.into());

		assert_eq!(pool.len(), 1);
		assert!(!pool.contains_unknown_block(&b1_hash));
		assert_eq!(pool.unknown_blocks().len(), 0);
	}

	#[test]
	fn orphan_block_pool_insert_unknown_block() {
		let mut pool = OrphanBlocksPool::new();
		let b1 = test_data::block_h1();
		let b1_hash = b1.hash();

		pool.insert_unknown_block(b1.into());

		assert_eq!(pool.len(), 1);
		assert!(pool.contains_unknown_block(&b1_hash));
		assert_eq!(pool.unknown_blocks().len(), 1);
	}

	#[test]
	fn orphan_block_pool_remove_known_blocks() {
		let mut pool = OrphanBlocksPool::new();
		let b1 = test_data::block_h1();
		let b1_hash = b1.hash();
		let b2 = test_data::block_h169();
		let b2_hash = b2.hash();

		pool.insert_orphaned_block(b1.into());
		pool.insert_unknown_block(b2.into());

		assert_eq!(pool.len(), 2);
		assert!(!pool.contains_unknown_block(&b1_hash));
		assert!(pool.contains_unknown_block(&b2_hash));
		assert_eq!(pool.unknown_blocks().len(), 1);

		pool.remove_known_blocks();

		assert_eq!(pool.len(), 1);
		assert!(!pool.contains_unknown_block(&b1_hash));
		assert!(pool.contains_unknown_block(&b2_hash));
		assert_eq!(pool.unknown_blocks().len(), 1);
	}

	#[test]
	fn orphan_block_pool_remove_blocks_for_parent() {
		let mut pool = OrphanBlocksPool::new();
		let b1 = test_data::block_h1();
		let b1_hash = b1.hash();
		let b2 = test_data::block_h169();
		let b2_hash = b2.hash();
		let b3 = test_data::block_h2();
		let b3_hash = b3.hash();

		pool.insert_orphaned_block(b1.into());
		pool.insert_unknown_block(b2.into());
		pool.insert_orphaned_block(b3.into());

		let removed = pool.remove_blocks_for_parent(&test_data::genesis().hash());
		assert_eq!(removed.len(), 2);
		assert_eq!(removed[0].hash(), &b1_hash);
		assert_eq!(removed[1].hash(), &b3_hash);

		assert_eq!(pool.len(), 1);
		assert!(!pool.contains_unknown_block(&b1_hash));
		assert!(pool.contains_unknown_block(&b2_hash));
		assert!(!pool.contains_unknown_block(&b1_hash));
		assert_eq!(pool.unknown_blocks().len(), 1);
	}

	#[test]
	fn orphan_block_pool_remove_blocks() {
		let mut pool = OrphanBlocksPool::new();
		let b1 = test_data::block_h1();
		let b1_hash = b1.hash();
		let b2 = test_data::block_h2();
		let b2_hash = b2.hash();
		let b3 = test_data::block_h169();
		let b3_hash = b3.hash();
		let b4 = test_data::block_h170();
		let b4_hash = b4.hash();
		let b5 = test_data::block_h181();

		pool.insert_orphaned_block(b1.into());
		pool.insert_orphaned_block(b2.into());
		pool.insert_orphaned_block(b3.into());
		pool.insert_orphaned_block(b4.into());
		pool.insert_orphaned_block(b5.into());

		let mut blocks_to_remove: HashSet<SHA256D> = HashSet::new();
		blocks_to_remove.insert(b1_hash.clone());
		blocks_to_remove.insert(b3_hash.clone());

		let removed = pool.remove_blocks(&blocks_to_remove);
		assert_eq!(removed.len(), 4);
		assert!(removed.iter().any(|h| h == &b1_hash));
		assert!(removed.iter().any(|h| h == &b2_hash));
		assert!(removed.iter().any(|h| h == &b3_hash));
		assert!(removed.iter().any(|h| h == &b4_hash));

		assert_eq!(pool.len(), 1);
	}
}
