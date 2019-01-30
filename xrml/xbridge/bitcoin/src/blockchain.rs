// Copyright 2018 Chainpool.

use primitives::hash::H256;
use rstd::marker::PhantomData;
use rstd::result::Result;
use runtime_io;
use runtime_support::StorageMap;
use tx::handle_tx;
use {BlockHeaderFor, BlockHeaderInfo, Trait};

pub enum ChainErr {
    /// Uknown parent
    UnknownParent,
    /// Not Found
    NotFound,
    /// Ancient fork
    AncientFork,
    OtherErr(&'static str),
}

impl ChainErr {
    pub fn info(&self) -> &'static str {
        match *self {
            ChainErr::UnknownParent => "Block parent is unknown",
            ChainErr::NotFound => "Not to find orphaned side chain in header collection; qed",
            ChainErr::AncientFork => "Fork is too long to proceed",
            ChainErr::OtherErr(s) => s,
        }
    }
}

pub struct Chain<T: Trait>(PhantomData<T>);

impl<T: Trait> Chain<T> {
    pub fn update_header(confirmed_header: BlockHeaderInfo) -> Result<(), ChainErr> {
        Self::canonize(&confirmed_header.header.hash())?;
        Ok(())
    }

    fn canonize(hash: &H256) -> Result<(), ChainErr> {
        let confirmed_header: BlockHeaderInfo = match <BlockHeaderFor<T>>::get(hash) {
            Some(header) => header,
            None => return Err(ChainErr::OtherErr("not found blockheader for this hash")),
        };

        runtime_io::print("[bridge-btc] confirmed header height:");
        runtime_io::print(confirmed_header.height as u64);

        let tx_list = confirmed_header.txid;
        for txid in tx_list {
            runtime_io::print("[bridge-btc] handle confirmed_header's tx list");
            // deposit & bind & withdraw & cert
            match handle_tx::<T>(&txid) {
                Err(_) => {
                    runtime_io::print("[bridge-btc] handle_tx error, tx hash:");
                    runtime_io::print(&txid[..]);
                }
                Ok(()) => (),
            }
        }
        Ok(())
    }
}
