use v1::traits::BlockChain;
use v1::types::{GetBlockResponse, VerboseBlock, RawBlock};
use v1::types::GetTxOutResponse;
use v1::types::GetTxOutSetInfoResponse;
use v1::types::H256;
use v1::types::U256;
use v1::helpers::errors::{block_not_found, block_at_height_not_found};
use jsonrpc_macros::Trailing;
use jsonrpc_core::Error;
use db;
use verification;
use ser::serialize;
use primitives::hash::H256 as GlobalH256;


pub struct BlockChainClient<T: BlockChainClientCoreApi> {
	core: T,
}

pub trait BlockChainClientCoreApi: Send + Sync + 'static {
	fn best_block_hash(&self) -> GlobalH256;
	fn block_hash(&self, height: u32) -> Option<GlobalH256>;
	fn difficulty(&self) -> f64;
	fn raw_block(&self, hash: GlobalH256) -> Option<RawBlock>;
	fn verbose_block(&self, hash: GlobalH256) -> Option<VerboseBlock>;
}

pub struct BlockChainClientCore {
	storage: db::SharedStore,
}

impl BlockChainClientCore {
	pub fn new(storage: db::SharedStore) -> Self {
		assert!(storage.best_block().is_some());
		
		BlockChainClientCore {
			storage: storage,
		}
	}
}

impl BlockChainClientCoreApi for BlockChainClientCore {
	fn best_block_hash(&self) -> GlobalH256 {
		self.storage.best_block().expect("storage with genesis block required").hash
	}

	fn block_hash(&self, height: u32) -> Option<GlobalH256> {
		self.storage.block_hash(height)
	}

	fn difficulty(&self) -> f64 {
		self.storage.difficulty()
	}

	fn raw_block(&self, hash: GlobalH256) -> Option<RawBlock> {
		self.storage.block(hash.into())
			.map(|block| {
				serialize(&block).into()
			})
	}

	fn verbose_block(&self, hash: GlobalH256) -> Option<VerboseBlock> {
		self.storage.block(hash.into())
			.map(|block| {
				let block: db::IndexedBlock = block.into();
				let height = self.storage.block_number(block.hash());
				let confirmations = match height {
					Some(block_number) => (self.storage.best_block().expect("genesis block is required").number - block_number + 1) as i64,
					None => -1,
				};
				let block_size = block.size();
				let median_time = verification::ChainVerifier::median_timestamp(self.storage.as_block_header_provider(), &block.header.raw);
				VerboseBlock {
					confirmations: confirmations,
					size: block_size as u32,
					strippedsize: block_size as u32, // TODO: segwit
					weight: block_size as u32, // TODO: segwit
					height: height,
					mediantime: median_time,
					difficulty: block.header.raw.bits.to_f64(),
					chainwork: U256::default(), // TODO: read from storage
					previousblockhash: Some(block.header.raw.previous_header_hash.clone().into()),
					nextblockhash: height.and_then(|h| self.storage.block_hash(h + 1).map(|h| h.into())),
					bits: block.header.raw.bits.into(),
					hash: block.hash().clone().into(),
					merkleroot: block.header.raw.merkle_root_hash.clone().into(),
					nonce: block.header.raw.nonce,
					time: block.header.raw.time,
					tx: block.transactions.into_iter().map(|t| t.hash.into()).collect(),
					version: block.header.raw.version,
					version_hex: format!("{:x}", &block.header.raw.version),
				}
			})
	}
}

impl<T> BlockChainClient<T> where T: BlockChainClientCoreApi {
	pub fn new(core: T) -> Self {
		BlockChainClient {
			core: core,
		}
	}
}

impl<T> BlockChain for BlockChainClient<T> where T: BlockChainClientCoreApi {
	fn best_block_hash(&self) -> Result<H256, Error> {
		Ok(self.core.best_block_hash().reversed().into())
	}

	fn block_hash(&self, height: u32) -> Result<H256, Error> {
		self.core.block_hash(height)
			.map(|h| h.reversed().into())
			.ok_or(block_at_height_not_found(height))
	}

	fn difficulty(&self) -> Result<f64, Error> {
		Ok(self.core.difficulty())
	}

	fn block(&self, hash: H256, verbose: Trailing<bool>) -> Result<GetBlockResponse, Error> {
		let global_hash: GlobalH256 = hash.clone().into();
		if verbose.0 {
			let verbose_block = self.core.verbose_block(global_hash.reversed());
			if let Some(mut verbose_block) = verbose_block {
				verbose_block.previousblockhash = verbose_block.previousblockhash.map(|h| h.reversed());
				verbose_block.nextblockhash = verbose_block.nextblockhash.map(|h| h.reversed());
				verbose_block.hash = verbose_block.hash.reversed();
				verbose_block.merkleroot = verbose_block.merkleroot.reversed();
				verbose_block.tx = verbose_block.tx.into_iter().map(|h| h.reversed()).collect();
				Some(GetBlockResponse::Verbose(verbose_block))
			} else {
				None
			}
		} else {
			self.core.raw_block(global_hash.reversed())
				.map(|block| GetBlockResponse::Raw(block))
		}
		.ok_or(block_not_found(hash))
	}

	fn transaction_out(&self, _transaction_hash: H256, _out_index: u32, _include_mempool: Trailing<bool>) -> Result<GetTxOutResponse, Error> {
		rpc_unimplemented!()
	}

	fn transaction_out_set_info(&self) -> Result<GetTxOutSetInfoResponse, Error> {
		rpc_unimplemented!()
	}
}

#[cfg(test)]
pub mod tests {
	use std::sync::Arc;
	use devtools::RandomTempPath;
	use jsonrpc_core::{IoHandler, GenericIoHandler};
	use db::{self, BlockStapler};
	use primitives::bytes::Bytes as GlobalBytes;
	use primitives::hash::H256 as GlobalH256;
	use v1::types::{VerboseBlock, RawBlock};
	use v1::traits::BlockChain;
	use test_data;
	use super::*;

	#[derive(Default)]
	struct SuccessBlockChainClientCore;
	#[derive(Default)]
	struct ErrorBlockChainClientCore;

	impl BlockChainClientCoreApi for SuccessBlockChainClientCore {
		fn best_block_hash(&self) -> GlobalH256 {
			test_data::genesis().hash()
		}

		fn block_hash(&self, _height: u32) -> Option<GlobalH256> {
			Some(test_data::genesis().hash())
		}

		fn difficulty(&self) -> f64 {
			1f64
		}

		fn raw_block(&self, _hash: GlobalH256) -> Option<RawBlock> {
			let b2_bytes: GlobalBytes = "010000004860eb18bf1b1620e37e9490fc8a427514416fd75159ab86688e9a8300000000d5fdcc541e25de1c7a5addedf24858b8bb665c9f36ef744ee42c316022c90f9bb0bc6649ffff001d08d2bd610101000000010000000000000000000000000000000000000000000000000000000000000000ffffffff0704ffff001d010bffffffff0100f2052a010000004341047211a824f55b505228e4c3d5194c1fcfaa15a456abdf37f9b9d97a4040afc073dee6c89064984f03385237d92167c13e236446b417ab79a0fcae412ae3316b77ac00000000".into();
			Some(RawBlock::from(b2_bytes))
		}

		fn verbose_block(&self, _hash: GlobalH256) -> Option<VerboseBlock> {
			// https://blockexplorer.com/block/000000006a625f06636b8bb6ac7b960a8d03705d1ace08b1a19da3fdcc99ddbd
			// https://blockchain.info/ru/block/000000006a625f06636b8bb6ac7b960a8d03705d1ace08b1a19da3fdcc99ddbd
			// https://webbtc.com/block/000000006a625f06636b8bb6ac7b960a8d03705d1ace08b1a19da3fdcc99ddbd.json
			Some(VerboseBlock {
				hash: "bddd99ccfda39da1b108ce1a5d70038d0a967bacb68b6b63065f626a00000000".into(),
				confirmations: 1, // h2
				size: 215,
				strippedsize: 215,
				weight: 215,
				height: Some(2),
				version: 1,
				version_hex: "1".to_owned(),
				merkleroot: "d5fdcc541e25de1c7a5addedf24858b8bb665c9f36ef744ee42c316022c90f9b".into(),
				tx: vec!["d5fdcc541e25de1c7a5addedf24858b8bb665c9f36ef744ee42c316022c90f9b".into()],
				time: 1231469744,
				mediantime: None,
				nonce: 1639830024,
				bits: 486604799,
				difficulty: 1.0,
				chainwork: 0.into(),
				previousblockhash: Some("4860eb18bf1b1620e37e9490fc8a427514416fd75159ab86688e9a8300000000".into()),
				nextblockhash: None,
			})
		}
	}

	impl BlockChainClientCoreApi for ErrorBlockChainClientCore {
		fn best_block_hash(&self) -> GlobalH256 {
			test_data::genesis().hash()
		}

		fn block_hash(&self, _height: u32) -> Option<GlobalH256> {
			None
		}

		fn difficulty(&self) -> f64 {
			1f64
		}

		fn raw_block(&self, _hash: GlobalH256) -> Option<RawBlock> {
			None
		}

		fn verbose_block(&self, _hash: GlobalH256) -> Option<VerboseBlock> {
			None
		}
	}

	#[test]
	fn best_block_hash_success() {
		let client = BlockChainClient::new(SuccessBlockChainClientCore::default());
		let handler = IoHandler::new();
		handler.add_delegate(client.to_delegate());

		let sample = handler.handle_request_sync(&(r#"
			{
				"jsonrpc": "2.0",
				"method": "getbestblockhash",
				"params": [],
				"id": 1
			}"#)).unwrap();

		// direct hash is 6fe28c0ab6f1b372c1a6a246ae63f74f931e8365e15a089c68d6190000000000
		// but client expects reverse hash
		assert_eq!(&sample, r#"{"jsonrpc":"2.0","result":"000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f","id":1}"#);
	}

	#[test]
	fn block_hash_success() {
		let client = BlockChainClient::new(SuccessBlockChainClientCore::default());
		let handler = IoHandler::new();
		handler.add_delegate(client.to_delegate());

		let sample = handler.handle_request_sync(&(r#"
			{
				"jsonrpc": "2.0",
				"method": "getblockhash",
				"params": [0],
				"id": 1
			}"#)).unwrap();

		// direct hash is 6fe28c0ab6f1b372c1a6a246ae63f74f931e8365e15a089c68d6190000000000
		// but client expects reverse hash
		assert_eq!(&sample, r#"{"jsonrpc":"2.0","result":"000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f","id":1}"#);
	}

	#[test]
	fn block_hash_error() {
		let client = BlockChainClient::new(ErrorBlockChainClientCore::default());
		let handler = IoHandler::new();
		handler.add_delegate(client.to_delegate());

		let sample = handler.handle_request_sync(&(r#"
			{
				"jsonrpc": "2.0",
				"method": "getblockhash",
				"params": [0],
				"id": 1
			}"#)).unwrap();

		assert_eq!(&sample, r#"{"jsonrpc":"2.0","error":{"code":-32099,"message":"Block at given height is not found","data":"0"},"id":1}"#);
	}

	#[test]
	fn difficulty_success() {
		let client = BlockChainClient::new(SuccessBlockChainClientCore::default());
		let handler = IoHandler::new();
		handler.add_delegate(client.to_delegate());

		let sample = handler.handle_request_sync(&(r#"
			{
				"jsonrpc": "2.0",
				"method": "getdifficulty",
				"params": [],
				"id": 1
			}"#)).unwrap();

		assert_eq!(&sample, r#"{"jsonrpc":"2.0","result":1.0,"id":1}"#);
	}

	#[test]
	fn verbose_block_contents() {
		let path = RandomTempPath::create_dir();
		let storage = Arc::new(db::Storage::new(path.as_path()).unwrap());
		storage.insert_block(&test_data::genesis()).expect("no error");
		storage.insert_block(&test_data::block_h1()).expect("no error");
		storage.insert_block(&test_data::block_h2()).expect("no error");

		let core = BlockChainClientCore::new(storage);

		// get info on block #1:
		// https://blockexplorer.com/block/00000000839a8e6886ab5951d76f411475428afc90947ee320161bbf18eb6048
		// https://blockchain.info/block/00000000839a8e6886ab5951d76f411475428afc90947ee320161bbf18eb6048
		// https://webbtc.com/block/00000000839a8e6886ab5951d76f411475428afc90947ee320161bbf18eb6048.json
		let verbose_block = core.verbose_block("4860eb18bf1b1620e37e9490fc8a427514416fd75159ab86688e9a8300000000".into());
		assert_eq!(verbose_block, Some(VerboseBlock {
			hash: "4860eb18bf1b1620e37e9490fc8a427514416fd75159ab86688e9a8300000000".into(),
			confirmations: 2, // h1 + h2
			size: 215,
			strippedsize: 215,
			weight: 215,
			height: Some(1),
			version: 1,
			version_hex: "1".to_owned(),
			merkleroot: "982051fd1e4ba744bbbe680e1fee14677ba1a3c3540bf7b1cdb606e857233e0e".into(),
			tx: vec!["982051fd1e4ba744bbbe680e1fee14677ba1a3c3540bf7b1cdb606e857233e0e".into()],
			time: 1231469665,
			mediantime: None,
			nonce: 2573394689,
			bits: 486604799,
			difficulty: 1.0,
			chainwork: 0.into(),
			previousblockhash: Some("6fe28c0ab6f1b372c1a6a246ae63f74f931e8365e15a089c68d6190000000000".into()),
			nextblockhash: Some("bddd99ccfda39da1b108ce1a5d70038d0a967bacb68b6b63065f626a00000000".into()),
		}));

		// get info on block #2:
		// https://blockexplorer.com/block/000000006a625f06636b8bb6ac7b960a8d03705d1ace08b1a19da3fdcc99ddbd
		// https://blockchain.info/ru/block/000000006a625f06636b8bb6ac7b960a8d03705d1ace08b1a19da3fdcc99ddbd
		// https://webbtc.com/block/000000006a625f06636b8bb6ac7b960a8d03705d1ace08b1a19da3fdcc99ddbd.json
		let verbose_block = core.verbose_block("bddd99ccfda39da1b108ce1a5d70038d0a967bacb68b6b63065f626a00000000".into());
		assert_eq!(verbose_block, Some(VerboseBlock {
			hash: "bddd99ccfda39da1b108ce1a5d70038d0a967bacb68b6b63065f626a00000000".into(),
			confirmations: 1, // h2
			size: 215,
			strippedsize: 215,
			weight: 215,
			height: Some(2),
			version: 1,
			version_hex: "1".to_owned(),
			merkleroot: "d5fdcc541e25de1c7a5addedf24858b8bb665c9f36ef744ee42c316022c90f9b".into(),
			tx: vec!["d5fdcc541e25de1c7a5addedf24858b8bb665c9f36ef744ee42c316022c90f9b".into()],
			time: 1231469744,
			mediantime: None,
			nonce: 1639830024,
			bits: 486604799,
			difficulty: 1.0,
			chainwork: 0.into(),
			previousblockhash: Some("4860eb18bf1b1620e37e9490fc8a427514416fd75159ab86688e9a8300000000".into()),
			nextblockhash: None,
		}));
	}

	#[test]
	fn raw_block_success() {
		let client = BlockChainClient::new(SuccessBlockChainClientCore::default());
		let handler = IoHandler::new();
		handler.add_delegate(client.to_delegate());

		let expected = r#"{"jsonrpc":"2.0","result":"010000004860eb18bf1b1620e37e9490fc8a427514416fd75159ab86688e9a8300000000d5fdcc541e25de1c7a5addedf24858b8bb665c9f36ef744ee42c316022c90f9bb0bc6649ffff001d08d2bd610101000000010000000000000000000000000000000000000000000000000000000000000000ffffffff0704ffff001d010bffffffff0100f2052a010000004341047211a824f55b505228e4c3d5194c1fcfaa15a456abdf37f9b9d97a4040afc073dee6c89064984f03385237d92167c13e236446b417ab79a0fcae412ae3316b77ac00000000","id":1}"#;

		let sample = handler.handle_request_sync(&(r#"
			{
				"jsonrpc": "2.0",
				"method": "getblock",
				"params": ["000000006a625f06636b8bb6ac7b960a8d03705d1ace08b1a19da3fdcc99ddbd", false],
				"id": 1
			}"#)).unwrap();
		assert_eq!(&sample, expected);

		// try without optional parameter
		let sample = handler.handle_request_sync(&(r#"
			{
				"jsonrpc": "2.0",
				"method": "getblock",
				"params": ["000000006a625f06636b8bb6ac7b960a8d03705d1ace08b1a19da3fdcc99ddbd"],
				"id": 1
			}"#)).unwrap();
		assert_eq!(&sample, expected);
	}

	#[test]
	fn raw_block_error() {
		let client = BlockChainClient::new(ErrorBlockChainClientCore::default());
		let handler = IoHandler::new();
		handler.add_delegate(client.to_delegate());

		let sample = handler.handle_request_sync(&(r#"
			{
				"jsonrpc": "2.0",
				"method": "getblock",
				"params": ["000000006a625f06636b8bb6ac7b960a8d03705d1ace08b1a19da3fdcc99ddbd", false],
				"id": 1
			}"#)).unwrap();

		assert_eq!(&sample, r#"{"jsonrpc":"2.0","error":{"code":-32099,"message":"Block with given hash is not found","data":"000000006a625f06636b8bb6ac7b960a8d03705d1ace08b1a19da3fdcc99ddbd"},"id":1}"#);
	}

	#[test]
	fn verbose_block_success() {
		let client = BlockChainClient::new(SuccessBlockChainClientCore::default());
		let handler = IoHandler::new();
		handler.add_delegate(client.to_delegate());

		let sample = handler.handle_request_sync(&(r#"
			{
				"jsonrpc": "2.0",
				"method": "getblock",
				"params": ["000000006a625f06636b8bb6ac7b960a8d03705d1ace08b1a19da3fdcc99ddbd",true],
				"id": 1
			}"#)).unwrap();

		assert_eq!(&sample, r#"{"jsonrpc":"2.0","result":{"bits":486604799,"chainwork":"","confirmations":1,"difficulty":1.0,"hash":"000000006a625f06636b8bb6ac7b960a8d03705d1ace08b1a19da3fdcc99ddbd","height":2,"mediantime":null,"merkleroot":"9b0fc92260312ce44e74ef369f5c66bbb85848f2eddd5a7a1cde251e54ccfdd5","nextblockhash":null,"nonce":1639830024,"previousblockhash":"00000000839a8e6886ab5951d76f411475428afc90947ee320161bbf18eb6048","size":215,"strippedsize":215,"time":1231469744,"tx":["9b0fc92260312ce44e74ef369f5c66bbb85848f2eddd5a7a1cde251e54ccfdd5"],"version":1,"versionHex":"1","weight":215},"id":1}"#);
	}

	#[test]
	fn verbose_block_error() {
		let client = BlockChainClient::new(ErrorBlockChainClientCore::default());
		let handler = IoHandler::new();
		handler.add_delegate(client.to_delegate());

		let sample = handler.handle_request_sync(&(r#"
			{
				"jsonrpc": "2.0",
				"method": "getblock",
				"params": ["000000006a625f06636b8bb6ac7b960a8d03705d1ace08b1a19da3fdcc99ddbd", true],
				"id": 1
			}"#)).unwrap();

		assert_eq!(&sample, r#"{"jsonrpc":"2.0","error":{"code":-32099,"message":"Block with given hash is not found","data":"000000006a625f06636b8bb6ac7b960a8d03705d1ace08b1a19da3fdcc99ddbd"},"id":1}"#);
	}
}
