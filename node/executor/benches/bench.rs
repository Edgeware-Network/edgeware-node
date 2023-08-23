// Copyright 2018-2020 Commonwealth Labs, Inc.
// This file is part of Edgeware.

// Edgeware is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Edgeware is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Edgeware.  If not, see <http://www.gnu.org/licenses/>.

use codec::{Decode, Encode};
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use kitchensink_runtime::{
	constants::currency::*, Block, BuildStorage, CheckedExtrinsic, GenesisConfig, Header,
	RuntimeCall, UncheckedExtrinsic,
};
use edgeware_executor::Executor;
use edgeware_primitives::{BlockNumber, Hash};
use edgeware_runtime::{
	constants::currency::*, Block, BuildStorage, Call, CheckedExtrinsic, GenesisConfig, Header, UncheckedExtrinsic,
};
use edgeware_testing::keyring::*;
use frame_support::Hashable;
use sc_executor::{
	Externalities, NativeElseWasmExecutor, RuntimeVersionOf, WasmExecutionMethod, WasmExecutor,
	WasmtimeInstantiationStrategy,
};
use sp_core::{
	storage::well_known_keys,
	traits::{CallContext, CodeExecutor, RuntimeCode},
};
use sp_runtime::traits::BlakeTwo256;
use sp_state_machine::TestExternalities as CoreTestExternalities;

criterion_group!(benches, bench_execute_block);
criterion_main!(benches);

/// The wasm runtime code.
pub fn compact_code_unwrap() -> &'static [u8] {
	edgeware_runtime::WASM_BINARY.expect(
		"Development wasm binary is not available. Testing is only supported with the flag \
		 disabled.",
	)
}

const GENESIS_HASH: [u8; 32] = [69u8; 32];

const TRANSACTION_VERSION: u32 = edgeware_runtime::VERSION.transaction_version;

const SPEC_VERSION: u32 = edgeware_runtime::VERSION.spec_version;

const HEAP_PAGES: u64 = 20;

type TestExternalities<H> = CoreTestExternalities<H>;

#[derive(Debug)]
enum ExecutionMethod {
	Native,
	Wasm(WasmExecutionMethod),
}

fn sign(xt: CheckedExtrinsic) -> UncheckedExtrinsic {
	edgeware_testing:::keyring::sign(xt, SPEC_VERSION, TRANSACTION_VERSION, GENESIS_HASH)
}

fn new_test_ext(genesis_config: &GenesisConfig) -> TestExternalities<BlakeTwo256> {
	let mut test_ext = TestExternalities::new_with_code(
		compact_code_unwrap(),
		genesis_config.build_storage().unwrap(),
	);
	test_ext
		.ext()
		.place_storage(well_known_keys::HEAP_PAGES.to_vec(), Some(HEAP_PAGES.encode()));
	test_ext
}

fn construct_block<E: Externalities>(
	executor: &NativeElseWasmExecutor<ExecutorDispatch>,
	ext: &mut E,
	number: BlockNumber,
	parent_hash: Hash,
	extrinsics: Vec<CheckedExtrinsic>,
) -> (Vec<u8>, Hash) {
	use sp_trie::{LayoutV0, TrieConfiguration};

	// sign extrinsics.
	let extrinsics = extrinsics.into_iter().map(sign).collect::<Vec<_>>();

	// calculate the header fields that we can.
	let extrinsics_root =
		LayoutV0::<BlakeTwo256>::ordered_trie_root(extrinsics.iter().map(Encode::encode))
			.to_fixed_bytes()
			.into();

	let header = Header {
		parent_hash,
		number,
		extrinsics_root,
		state_root: Default::default(),
		digest: Default::default(),
	};

	let runtime_code = RuntimeCode {
		code_fetcher: &sp_core::traits::WrappedRuntimeCode(compact_code_unwrap().into()),
		hash: vec![1, 2, 3],
		heap_pages: None,
	};

	// execute the block to get the real header.
	executor
		.call(
			ext,
			&runtime_code,
			"Core_initialize_block",
			&header.encode(),
			true,
			CallContext::Offchain,
		)
		.0
		.unwrap();

	for i in extrinsics.iter() {
		executor
			.call(
				ext,
				&runtime_code,
				"BlockBuilder_apply_extrinsic",
				&i.encode(),
				true,
				CallContext::Offchain,
			)
			.0
			.unwrap();
	}

	let header = Header::decode(
		&mut &executor
			.call(
				ext,
				&runtime_code,
				"BlockBuilder_finalize_block",
				&[0u8; 0],
				true,
				CallContext::Offchain,
			)
			.0
			.unwrap()[..],
	)
	.unwrap();

	let hash = header.blake2_256();
	(Block { header, extrinsics }.encode(), hash.into())
}

fn test_blocks(
	genesis_config: &GenesisConfig,
	executor: &NativeElseWasmExecutor<ExecutorDispatch>,
) -> Vec<(Vec<u8>, Hash)> {
	let mut test_ext = new_test_ext(genesis_config);
	let mut block1_extrinsics = vec![CheckedExtrinsic {
		signed: None,
		function: RuntimeCall::Timestamp(pallet_timestamp::Call::set { now: 0 }),
	}];
	block1_extrinsics.extend((0..20).map(|i| CheckedExtrinsic {
		signed: Some((alice(), signed_extra(i, 0))),
		function: RuntimeCall::Balances(pallet_balances::Call::transfer_allow_death {
			dest: bob().into(),
			value: 1 * DOLLARS,
		}),
	}));
	let block1 =
		construct_block(executor, &mut test_ext.ext(), 1, GENESIS_HASH.into(), block1_extrinsics);

	vec![block1]
}

fn bench_execute_block(c: &mut Criterion) {
	let mut group = c.benchmark_group("execute blocks");
	let execution_methods = vec![
		ExecutionMethod::Native,
		ExecutionMethod::Wasm(WasmExecutionMethod::Compiled {
			instantiation_strategy: WasmtimeInstantiationStrategy::PoolingCopyOnWrite,
		}),
	];

	for strategy in execution_methods {
		group.bench_function(format!("{:?}", strategy), |b| {
			let genesis_config = edgeware_testing::genesis::config(Some(compact_code_unwrap()));
			let use_native = match strategy {
				ExecutionMethod::Native => true,
				ExecutionMethod::Wasm(..) => false,
			};

			let executor =
				NativeElseWasmExecutor::new_with_wasm_executor(WasmExecutor::builder().build());
			let runtime_code = RuntimeCode {
				code_fetcher: &sp_core::traits::WrappedRuntimeCode(compact_code_unwrap().into()),
				hash: vec![1, 2, 3],
				heap_pages: None,
			};

			// Get the runtime version to initialize the runtimes cache.
			{
				let mut test_ext = new_test_ext(&genesis_config);
				executor.runtime_version(&mut test_ext.ext(), &runtime_code).unwrap();
			}

			let blocks = test_blocks(&genesis_config, &executor);

			b.iter_batched_ref(
				|| new_test_ext(&genesis_config),
				|test_ext| {
					for block in blocks.iter() {
						executor
							.call(
								&mut test_ext.ext(),
								&runtime_code,
								"Core_execute_block",
								&block.0,
								use_native,
								CallContext::Offchain,
							)
							.0
							.unwrap();
					}
				},
				BatchSize::LargeInput,
			);
		});
	}
	