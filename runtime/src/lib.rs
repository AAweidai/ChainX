// Copyright 2018 Chainpool.

//! The ChainX runtime. This can be compiled with ``#[no_std]`, ready for Wasm.

#![cfg_attr(not(feature = "std"), no_std)]
// `construct_runtime!` does a lot of recursion and requires us to increase the limit to 256.
#![recursion_limit = "256"]

extern crate parity_codec as codec;
#[macro_use]
extern crate parity_codec_derive;
#[cfg(feature = "std")]
extern crate serde;

#[macro_use]
extern crate substrate_client as client;
extern crate substrate_consensus_aura_primitives as consensus_aura;
extern crate substrate_primitives as primitives;

#[macro_use]
extern crate sr_primitives as runtime_primitives;
#[macro_use]
extern crate sr_version as version;
#[cfg_attr(not(feature = "std"), macro_use)]
extern crate sr_std as rstd;

// substrate runtime module
#[macro_use]
extern crate srml_support;
extern crate srml_aura as aura;
extern crate srml_balances as balances;
extern crate srml_consensus as consensus;
extern crate srml_grandpa as grandpa;
extern crate srml_session as session;
extern crate srml_system as system;
extern crate srml_timestamp as timestamp;
// unused
extern crate srml_contract as contract;
extern crate srml_council as council;
extern crate srml_democracy as democracy;
extern crate srml_treasury as treasury;

// chainx
extern crate chainx_primitives;
// chainx runtime module
extern crate xrml_executive as xexective;
extern crate xrml_xsystem as xsystem;
// fee;
extern crate xrml_fee_manager as fee_manager;
// assets;
extern crate xrml_xassets_assets as xassets;

pub use balances::address::Address as RawAddress;
use consensus_aura::api as aura_api;
pub use runtime_primitives::{Perbill, Permill};

#[cfg(feature = "std")]
use council::{motions as council_motions, voting as council_voting};
use grandpa::fg_primitives::{self, ScheduledChange};
use rstd::prelude::*;

use primitives::u32_trait::{_2, _4};
use primitives::OpaqueMetadata;

use runtime_primitives::generic;
use runtime_primitives::traits::{BlakeTwo256, Block as BlockT, Convert, DigestFor, NumberFor};
//#[cfg(feature = "std")]
//use council::{motions as council_motions, voting as council_voting};
use runtime_primitives::transaction_validity::TransactionValidity;
use runtime_primitives::{ApplyResult, BasicInherentData, CheckInherentError};
#[cfg(any(feature = "std", test))]
use version::NativeVersion;
use version::RuntimeVersion;

use client::{block_builder::api as block_builder_api, runtime_api as client_api};

use srml_support::inherent::ProvideInherent;

// for set consensus period
pub use srml_support::{RuntimeMetadata, StorageValue};
pub use timestamp::BlockPeriod;
pub use timestamp::Call as TimestampCall;

use chainx_primitives::{
    AccountId, AccountIndex, Balance, BlockNumber, Hash, Index, SessionKey, Signature,
};

#[cfg(any(feature = "std", test))]
pub use runtime_primitives::BuildStorage;

/// The position of the timestamp set extrinsic.
pub const TIMESTAMP_SET_POSITION: u32 = 0;
/// The position of the offline nodes noting extrinsic.
pub const NOTE_OFFLINE_POSITION: u32 = 1;

pub const BLOCK_PRODUCER_POSITION: u32 = 1;

/// Runtime version.
pub const VERSION: RuntimeVersion = RuntimeVersion {
    spec_name: create_runtime_str!("chainx"),
    impl_name: create_runtime_str!("chainx-net"),
    authoring_version: 1,
    spec_version: 1,
    impl_version: 0,
    apis: RUNTIME_API_VERSIONS,
};

/// Native version.
#[cfg(any(feature = "std", test))]
pub fn native_version() -> NativeVersion {
    NativeVersion {
        runtime_version: VERSION,
        can_author_with: Default::default(),
    }
}

impl system::Trait for Runtime {
    type Origin = Origin;
    type Index = Index;
    type BlockNumber = BlockNumber;
    type Hash = Hash;
    type Hashing = BlakeTwo256;
    type Digest = generic::Digest<Log>;
    type AccountId = AccountId;
    type Header = Header;
    type Event = Event;
    type Log = Log;
}

impl balances::Trait for Runtime {
    type Balance = Balance;
    type AccountIndex = AccountIndex;
    //    type OnFreeBalanceZero = (Staking, Contract);
    //    type EnsureAccountLiquid = Staking;
    type OnFreeBalanceZero = ();
    type EnsureAccountLiquid = ();
    type Event = Event;
}

impl timestamp::Trait for Runtime {
    const TIMESTAMP_SET_POSITION: u32 = TIMESTAMP_SET_POSITION;
    type Moment = u64;
    type OnTimestampSet = Aura;
}

impl consensus::Trait for Runtime {
    const NOTE_OFFLINE_POSITION: u32 = NOTE_OFFLINE_POSITION;
    type Log = Log;
    type SessionKey = SessionKey;
    type InherentOfflineReport = ();
}

/// Session key conversion.
pub struct SessionKeyConversion;

impl Convert<AccountId, SessionKey> for SessionKeyConversion {
    fn convert(a: AccountId) -> SessionKey {
        a.to_fixed_bytes().into()
    }
}

impl session::Trait for Runtime {
    type ConvertAccountIdToSessionKey = SessionKeyConversion;
    //    type OnSessionChange = (Staking, grandpa::SyncedAuthorities<Runtime>);
    type OnSessionChange = grandpa::SyncedAuthorities<Runtime>;
    type Event = Event;
}

impl grandpa::Trait for Runtime {
    type SessionKey = SessionKey;
    type Log = Log;
    type Event = Event;
}

impl aura::Trait for Runtime {
    //    type HandleReport = aura::StakingSlasher<Runtime>;
    type HandleReport = ();
}

//impl treasury::Trait for Runtime {
//    type ApproveOrigin = council_motions::EnsureMembers<_4>;
//    type RejectOrigin = council_motions::EnsureMembers<_2>;
//    type Event = Event;
//}
//
//impl democracy::Trait for Runtime {
//    type Proposal = Call;
//    type Event = Event;
//}
//
//impl council::Trait for Runtime {
//    type Event = Event;
//}
//
//impl contract::Trait for Runtime {
//    type DetermineContractAddress = contract::SimpleAddressDeterminator<Runtime>;
//    type Gas = u64;
//    type Event = Event;
//}
//
//// TODO add voting and motions at here
//impl council::voting::Trait for Runtime {
//    type Event = Event;
//}
//
//impl council::motions::Trait for Runtime {
//    type Origin = Origin;
//    type Proposal = Call;
//    type Event = Event;
//}

// cxrml trait
impl xsystem::Trait for Runtime {
    const XSYSTEM_SET_POSITION: u32 = 3;
}
// fees
impl fee_manager::Trait for Runtime {
    //    type Event = Event;
}
// assets
impl xassets::Trait for Runtime {
    type Event = Event;
    type OnAssetChanged = ();
}

construct_runtime!(
    pub enum Runtime with Log(InternalLog: DigestItem<Hash, SessionKey>) where
        Block = Block,
        NodeBlock = chainx_primitives::Block,
        InherentData = BasicInherentData
    {
        System: system::{default, Log(ChangesTrieRoot)},
        Balances: balances,
        Timestamp: timestamp::{Module, Call, Storage, Config<T>, Inherent},
        Consensus: consensus::{Module, Call, Storage, Config<T>, Log(AuthoritiesChange), Inherent},
        Session: session,
        Grandpa: grandpa::{Module, Call, Storage, Config<T>, Log(), Event<T>},
        Aura: aura::{Module},

        // chainx runtime module
        XSystem: xsystem::{Module, Call, Storage, Config<T>}, //, Inherent},
        // fee
        XFeeManager: fee_manager::{Module, Call, Storage, Config<T>},
        // assets
        XAssets: xassets,
    }
);

/// The address format for describing accounts.
pub type Address = balances::Address<Runtime>;
/// Block header type as expected by this runtime.
pub type Header = generic::Header<BlockNumber, BlakeTwo256, Log>;
/// Block type as expected by this runtime.
pub type Block = generic::Block<Header, UncheckedExtrinsic>;
/// BlockId type as expected by this runtime.
pub type BlockId = generic::BlockId<Block>;
/// Unchecked extrinsic type as expected by this runtime.
pub type UncheckedExtrinsic = generic::UncheckedMortalExtrinsic<Address, Index, Call, Signature>;
/// Executive: handles dispatch to the various modules.
pub type Executive =
    xexective::Executive<Runtime, Block, balances::ChainContext<Runtime>, XFeeManager, AllModules>;

// define tokenbalances module type
//pub type TokenBalance = u128;

impl_runtime_apis! {
    impl client_api::Core<Block> for Runtime {
        fn version() -> RuntimeVersion {
            VERSION
        }

        fn authorities() -> Vec<SessionKey> {
            Consensus::authorities()
        }

        fn execute_block(block: Block) {
            Executive::execute_block(block)
        }

        fn initialise_block(header: <Block as BlockT>::Header) {
            Executive::initialise_block(&header)
        }
    }

    impl client_api::Metadata<Block> for Runtime {
        fn metadata() -> OpaqueMetadata {
            Runtime::metadata().into()
        }
    }

    impl block_builder_api::BlockBuilder<Block, BasicInherentData> for Runtime {
        fn apply_extrinsic(extrinsic: <Block as BlockT>::Extrinsic) -> ApplyResult {
            Executive::apply_extrinsic(extrinsic)
        }

        fn finalise_block() -> <Block as BlockT>::Header {
            Executive::finalise_block()
        }

        fn inherent_extrinsics(data: BasicInherentData) -> Vec<<Block as BlockT>::Extrinsic> {
            let mut inherent = Vec::new();

            inherent.extend(
                Timestamp::create_inherent_extrinsics(data.timestamp)
                    .into_iter()
                    .map(|v| (v.0, UncheckedExtrinsic::new_unsigned(Call::Timestamp(v.1))))
            );

            inherent.extend(
                Consensus::create_inherent_extrinsics(data.consensus)
                    .into_iter()
                    .map(|v| (v.0, UncheckedExtrinsic::new_unsigned(Call::Consensus(v.1))))
            );

            // TODO add blockproducer

            inherent.as_mut_slice().sort_unstable_by_key(|v| v.0);
            inherent.into_iter().map(|v| v.1).collect()
        }

        fn check_inherents(block: Block, data: BasicInherentData) -> Result<(), CheckInherentError> {
            let expected_slot = data.aura_expected_slot;

            // draw timestamp out from extrinsics.
            let set_timestamp = block.extrinsics()
                .get(TIMESTAMP_SET_POSITION as usize)
                .and_then(|xt: &UncheckedExtrinsic| match xt.function {
                    Call::Timestamp(TimestampCall::set(ref t)) => Some(t.clone()),
                    _ => None,
                })
                .ok_or_else(|| CheckInherentError::Other("No valid timestamp in block.".into()))?;

            // take the "worse" result of normal verification and the timestamp vs. seal
            // check.
            CheckInherentError::combine_results(
                Runtime::check_inherents(block, data),
                || {
                    Aura::verify_inherent(set_timestamp.into(), expected_slot)
                        .map_err(|s| CheckInherentError::Other(s.into()))
                },
            )
        }

        fn random_seed() -> <Block as BlockT>::Hash {
            System::random_seed()
        }
    }

    impl client_api::TaggedTransactionQueue<Block> for Runtime {
        fn validate_transaction(tx: <Block as BlockT>::Extrinsic) -> TransactionValidity {
            Executive::validate_transaction(tx)
        }
    }

    impl fg_primitives::GrandpaApi<Block> for Runtime {
        fn grandpa_pending_change(digest: DigestFor<Block>)
            -> Option<ScheduledChange<NumberFor<Block>>>
        {
            for log in digest.logs.iter().filter_map(|l| match l {
                Log(InternalLog::grandpa(grandpa_signal)) => Some(grandpa_signal),
                _=> None
            }) {
                if let Some(change) = Grandpa::scrape_digest_change(log) {
                    return Some(change);
                }
            }
            None
        }

        fn grandpa_authorities() -> Vec<(SessionKey, u64)> {
            Grandpa::grandpa_authorities()
        }
    }

    impl aura_api::AuraApi<Block> for Runtime {
        fn slot_duration() -> u64 {
            Aura::slot_duration()
        }
    }
}
