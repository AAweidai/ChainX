use rstd::prelude::*;
use rstd::result::Result;
use rstd::marker::PhantomData;
use runtime_support::{StorageMap, StorageValue};
use runtime_primitives::traits::As;
use {IrrBlock, BtcFee, NetworkId, AddressMap};
use runtime_io;

use primitives::hash::H256;
use chain::BlockHeader;
use finacial_recordes::Symbol;
use finacial_recordes;
use script::Script;

use {Trait, BlockHeaderFor, BestIndex, NumberForHash, HashsForNumber, ParamsInfo, AccountMap,
     Params, DepositCache, TxProposal};

use tx::{TxStorage, RollBack, Proposal};


#[derive(PartialEq, Eq, Clone, Encode, Decode, Default)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct BestHeader {
    /// Height/number of the best block (genesis block has zero height)
    pub number: u32,
    /// Hash of the best block
    pub hash: H256,
}

#[derive(Clone)]
pub struct SideChainOrigin {
    /// newest ancestor block number
    pub ancestor: u32,
    /// side chain block hashes. Ordered from oldest to newest
    pub canonized_route: Vec<H256>,
    /// canon chain block hahses. Ordered from oldest to newest
    pub decanonized_route: Vec<H256>,
    /// new block number
    pub block_number: u32,
}

#[derive(Clone)]
pub enum BlockOrigin {
    KnownBlock,
    CanonChain { block_number: u32 },
    SideChain(SideChainOrigin),
    SideChainBecomingCanonChain(SideChainOrigin),
}

pub enum ChainErr {
    /// Invalid block
    CannotCanonize,
    /// Uknown parent
    UnknownParent,
    /// Not Found
    NotFound,
    /// Ancient fork
    AncientFork,
    /// unreachable,
    Unreachable,
    /// must zero
    CanonizeMustZero,
    DecanonizeMustZero,
    ForkErr,

    OtherErr(&'static str),
}

impl ChainErr {
    pub fn info(&self) -> &'static str {
        match *self {
            ChainErr::CannotCanonize => "Cannot canonize block",
            ChainErr::UnknownParent => "Block parent is unknown",
            ChainErr::NotFound => "not to find orphaned side chain in header collection; qed",
            ChainErr::AncientFork => "Fork is too long to proceed",
            ChainErr::Unreachable => "should not occur",
            ChainErr::CanonizeMustZero => "[canonize] must be zero in this case",
            ChainErr::DecanonizeMustZero => "[decanonize] must be zero in this case",
            ChainErr::ForkErr => "the hash should same",
            ChainErr::OtherErr(s) => s,
        }
    }
}

pub struct Chain<T: Trait>(PhantomData<T>);

impl<T: Trait> Chain<T> {
    pub fn insert_best_header(header: BlockHeader) -> Result<(), ChainErr> {
        let block_origin = Self::block_origin(&header)?;
        match block_origin {
            BlockOrigin::KnownBlock => {
                return Err(ChainErr::Unreachable);
            }
            // case 1: block has been added to the main branch
            BlockOrigin::CanonChain { .. } => {
                Self::canonize(&header.hash())?;
                Ok(())
            }
            // case 2: block has been added to the side branch with reorganization to this branch
            BlockOrigin::SideChainBecomingCanonChain(origin) => {
                Self::fork(origin.clone())?;
                Self::canonize(&header.hash())?;
                Ok(())
            }
            // case 3: block has been added to the side branch without reorganization to this branch
            BlockOrigin::SideChain(_origin) => Ok(()),
        }
    }

    fn decanonize() -> Result<H256, ChainErr> {
        let best_index = <BestIndex<T>>::get();
        let best_hash = best_index.hash;
        let best_bumber = best_index.number;

        //todo change unwrap
        let (best_header, _, _): (BlockHeader, T::AccountId, T::BlockNumber) =
            <BlockHeaderFor<T>>::get(&best_hash).unwrap();
        let new_best_header = BestHeader {
            hash: best_header.previous_header_hash.clone(),
            number: if best_bumber > 0 {
                best_bumber - 1
            } else {
                if best_header.previous_header_hash != Default::default() {
                    return Err(ChainErr::DecanonizeMustZero);
                }
                0
            },
        };

        // remove related tx
        TxStorage::<T>::rollback_tx(&best_hash).map_err(|s| {
            ChainErr::OtherErr(s)
        })?;

        <NumberForHash<T>>::remove(&best_hash);
        // do not need to remove HashsForNumber

        <BestIndex<T>>::put(new_best_header);
        Ok(best_hash)
    }

    fn canonize(hash: &H256) -> Result<(), ChainErr> {
        // get current best index
        let best_index = <BestIndex<T>>::get();
        let best_hash = best_index.hash;
        let best_number = best_index.number;

        //todo change unwrap
        let (header, _, _): (BlockHeader, T::AccountId, T::BlockNumber) =
            <BlockHeaderFor<T>>::get(hash).unwrap();
        if best_hash != header.previous_header_hash {
            return Err(ChainErr::CannotCanonize);
        }

        let new_best_header = BestHeader {
            hash: hash.clone(),
            number: if header.previous_header_hash == Default::default() {
                if best_number != 0 {
                    return Err(ChainErr::CanonizeMustZero);
                }
                0
            } else {
                best_number + 1
            },
        };

        let symbol: Symbol = b"x-btc".to_vec();
        let irr_block = <IrrBlock<T>>::get();
        // Deposit
        if let Some(vec) = <DepositCache<T>>::take() {
            runtime_io::print("------DepositCache take");
            let mut uncomplete_cache: Vec<(T::AccountId, u64, H256)> = Vec::new();
            for (account_id, amount, block_hash) in vec {
                match <NumberForHash<T>>::get(block_hash.clone()) {
                    Some(height) => {
                        if new_best_header.number > height + irr_block {
                            runtime_io::print("------finacial_recordes deposit");
                            <finacial_recordes::Module<T>>::deposit(
                                &account_id,
                                &symbol,
                                As::sa(amount),
                            );
                        } else {
                            uncomplete_cache.push((account_id, amount, block_hash));
                        }
                    }
                    None => {
                        uncomplete_cache.push((account_id, amount, block_hash));
                    } // Optmise
                }
            }
            <DepositCache<T>>::put(uncomplete_cache);
        }

        // Withdraw
        let candidate = <TxProposal<T>>::get();
        if candidate.is_some() {
            let tx = candidate.unwrap();
            match <NumberForHash<T>>::get(tx.block_hash) {
                Some(height) => {
                    if new_best_header.number > height + irr_block {
                        for output in tx.tx.outputs.iter() {
                            let script: Script = output.clone().script_pubkey.into();
                            let script_address =
                                script.extract_destinations().unwrap_or(Vec::new());
                            let network_id = <NetworkId<T>>::get();
                            let network = if network_id == 1 {
                                keys::Network::Testnet
                            } else {
                                keys::Network::Mainnet
                            };
                            let address = keys::Address {
                                kind: script_address[0].kind,
                                network: network,
                                hash: script_address[0].hash.clone(),
                            };
                            let account_id = <AddressMap<T>>::get(address);
                            if account_id.is_some() {
                                <finacial_recordes::Module<T>>::withdrawal_finish(
                                    &account_id.unwrap(),
                                    &symbol,
                                    true,
                                );
                            }
                        }
                        let vec = <finacial_recordes::Module<T>>::get_withdraw_cache(&symbol);
                        if vec.is_some() {
                            let mut address_vec = Vec::new();
                            for (account_id, balance) in vec.unwrap() {
                                let address = <AccountMap<T>>::get(account_id);
                                if address.is_some() {
                                    address_vec.push((address.unwrap(), balance.as_() as u64));
                                }
                            }
                            let btc_fee = <BtcFee<T>>::get();
                            let _ = <Proposal<T>>::create_proposal(address_vec, btc_fee);
                        } else {
                            <TxProposal<T>>::kill();
                        }
                    }
                }
                None => {}
            }
        }

        <NumberForHash<T>>::insert(new_best_header.hash.clone(), new_best_header.number);
        runtime_io::print("------------");
        runtime_io::print(new_best_header.hash.to_vec().as_slice());
        <HashsForNumber<T>>::mutate(new_best_header.number, |v| {
            let h = new_best_header.hash.clone();
            if v.contains(&h) == false {
                v.push(h);
            }
        });

        <BestIndex<T>>::put(new_best_header);
        Ok(())
    }
    /// Rollbacks single best block
    #[allow(unused)]
    pub fn rollback_best() -> Result<H256, ChainErr> {
        Self::decanonize()
    }

    fn fork(side_chain: SideChainOrigin) -> Result<(), ChainErr> {
        for hash in side_chain.decanonized_route.into_iter().rev() {
            let decanonized_hash = Self::decanonize()?;

            if hash != decanonized_hash {
                return Err(ChainErr::ForkErr);
            }
        }

        for block_hash in &side_chain.canonized_route {
            Self::canonize(block_hash)?;
        }

        Ok(())
    }

    fn block_origin(header: &BlockHeader) -> Result<BlockOrigin, ChainErr> {
        let best_index: BestHeader = <BestIndex<T>>::get();
        // TODO change unwrap
        let (best_header, _, _): (BlockHeader, T::AccountId, T::BlockNumber) =
            <BlockHeaderFor<T>>::get(&best_index.hash).unwrap();
        if <NumberForHash<T>>::exists(header.hash()) {
            return Ok(BlockOrigin::KnownBlock);
        }

        if best_header.hash() == header.previous_header_hash {
            return Ok(BlockOrigin::CanonChain {
                block_number: best_index.number + 1,
            });
        }

        if <BlockHeaderFor<T>>::exists(&header.previous_header_hash) == false {
            return Err(ChainErr::UnknownParent);
        }

        let params: Params = <ParamsInfo<T>>::get();

        let mut sidechain_route = Vec::new();
        let mut next_hash = header.previous_header_hash.clone();
        for fork_len in 0..params.max_fork_route_preset {
            let num = <NumberForHash<T>>::get(&next_hash);
            match num {
                None => {
                    sidechain_route.push(next_hash.clone());
                    if let Some(header) = <BlockHeaderFor<T>>::get(&next_hash) {
                        next_hash = header.0.previous_header_hash;
                    } else {
                        return Err(ChainErr::NotFound);
                    }
                }
                Some(number) => {
                    // find common ancestor in main chain
                    let block_number = number + fork_len + 1;
                    let origin = SideChainOrigin {
                        ancestor: number,
                        canonized_route: sidechain_route.into_iter().rev().collect(),
                        decanonized_route: (number + 1..best_index.number + 1)
                            .into_iter()
                            .filter_map(|decanonized_bn| {
                                let hash_list = <HashsForNumber<T>>::get(decanonized_bn);
                                for h in hash_list {
                                    // look up in main chain
                                    if <NumberForHash<T>>::get(&h).is_some() {
                                        return Some(h);
                                    }
                                }
                                None
                            })
                            .collect(),
                        block_number: block_number,
                    };
                    if block_number > best_index.number {
                        return Ok(BlockOrigin::SideChainBecomingCanonChain(origin));
                    } else {
                        return Ok(BlockOrigin::SideChain(origin));
                    }
                }
            }
        }

        Err(ChainErr::AncientFork)
    }
}
