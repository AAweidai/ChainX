// Copyright 2018 Chainpool.
//! Test utilities

#![cfg(test)]

use super::*;
use runtime_io;
use runtime_io::with_externalities;
use runtime_primitives::testing::{Digest, DigestItem, Header};
use runtime_primitives::traits::{BlakeTwo256, Identity};
use runtime_primitives::BuildStorage;
use substrate_primitives::{Blake2Hasher, H256};
use {balances, consensus, session, system, timestamp, xassets, GenesisConfig, Module, Trait};

impl_outer_origin! {
    pub enum Origin for Test {}
}

// Workaround for https://github.com/rust-lang/rust/issues/26925 . Remove when sorted.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Test;
impl consensus::Trait for Test {
    const NOTE_OFFLINE_POSITION: u32 = 1;
    type Log = DigestItem;
    type SessionKey = u64;
    type InherentOfflineReport = ();
}
impl system::Trait for Test {
    type Origin = Origin;
    type Index = u64;
    type BlockNumber = u64;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type Digest = Digest;
    type AccountId = u64;
    type Header = Header;
    type Event = ();
    type Log = DigestItem;
}
impl balances::Trait for Test {
    type Balance = u64;
    type AccountIndex = u64;
    type OnFreeBalanceZero = ();
    type EnsureAccountLiquid = ();
    type Event = ();
}
impl xassets::Trait for Test {
    type Event = ();
    type OnAssetChanged = ();
}
impl timestamp::Trait for Test {
    const TIMESTAMP_SET_POSITION: u32 = 0;
    type Moment = u64;
    type OnTimestampSet = ();
}
impl session::Trait for Test {
    type ConvertAccountIdToSessionKey = Identity;
    type OnSessionChange = ();
    type Event = ();
}
impl Trait for Test {}

pub fn new_test_ext() -> runtime_io::TestExternalities<Blake2Hasher> {
    let mut t = system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap()
        .0;
    t.extend(
        consensus::GenesisConfig::<Test> {
            code: vec![],
            authorities: vec![],
        }
        .build_storage()
        .unwrap()
        .0,
    );
    t.extend(
        session::GenesisConfig::<Test> {
            session_length: 1,
            validators: vec![10, 20],
        }
        .build_storage()
        .unwrap()
        .0,
    );
    t.extend(
        balances::GenesisConfig::<Test> {
            balances: vec![(1, 10), (2, 20), (3, 30), (4, 40), (10, 100), (20, 100)],
            transaction_base_fee: 0,
            transaction_byte_fee: 0,
            existential_deposit: 0,
            transfer_fee: 0,
            creation_fee: 0,
            reclaim_rebate: 0,
        }
        .build_storage()
        .unwrap()
        .0,
    );
    t.extend(
        GenesisConfig::<Test> {
            _genesis_phantom_data: ::std::marker::PhantomData::<Test>,
            shares_per_cert: 50,
            activation_per_share: 100_000_000,
            maximum_cert_count: 178,
            total_issued: 2,
        }
        .build_storage()
        .unwrap()
        .0,
    );
    runtime_io::TestExternalities::new(t)
}

pub type System = system::Module<Test>;
pub type XAccounts = Module<Test>;

#[test]
fn issue_should_work() {
    with_externalities(&mut new_test_ext(), || {
        System::set_block_number(10);
        assert_ok!(XAccounts::issue(b"alice".to_vec(), 1, 1));
        assert_eq!(XAccounts::total_issued(), 3);
        assert_eq!(
            XAccounts::cert_immutable_props_of(b"alice".to_vec()),
            CertImmutableProps {
                issued_at: 10,
                frozen_duration: 1
            }
        );
        assert_eq!(XAccounts::remaining_shares_of(b"alice".to_vec()), 50);
        assert_noop!(
            XAccounts::issue(b"alice".to_vec(), 1, 1),
            "Cert name already exists."
        );
    });
}
