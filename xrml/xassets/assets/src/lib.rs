// Copyright 2018 Chainpool.
//! Assets: Handles token asset balances.

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

#[macro_use]
extern crate parity_codec_derive;
extern crate parity_codec as codec;

// for substrate
extern crate substrate_primitives;

// for substrate runtime
extern crate sr_std as rstd;

extern crate sr_io as runtime_io;
extern crate sr_primitives as primitives;

// for substrate runtime module lib
#[macro_use]
extern crate srml_support as runtime_support;
extern crate srml_balances as balances;
extern crate srml_system as system;

extern crate xr_primitives;

extern crate xrml_xsupport as xsupport;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod assetdef;
pub mod memo;

use primitives::traits::{CheckedAdd, CheckedSub, StaticLookup, Zero};
use rstd::collections::btree_map::BTreeMap;
use rstd::iter::FromIterator;
use rstd::prelude::*;
use rstd::result::Result as StdResult;
use rstd::slice::Iter;
use runtime_support::dispatch::Result;
use runtime_support::{StorageMap, StorageValue};

// substrate mod
use system::ensure_signed;

use xsupport::storage::btree_map::CodecBTreeMap;

pub use assetdef::{
    is_valid_desc, is_valid_token, Asset, Chain, ChainT, Desc, DescString, Precision, Token,
    TokenString,
};

pub use memo::{is_valid_memo, Memo};

pub trait Trait: balances::Trait {
    /// Event
    type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;

    type OnAssetChanged: OnAssetChanged<Self::AccountId, Self::Balance>;

    type OnAssetRegisterOrRevoke: OnAssetRegisterOrRevoke;
}

pub trait OnAssetChanged<AccountId, Balance> {
    fn on_move(
        token: &Token,
        from: &AccountId,
        from_type: AssetType,
        to: &AccountId,
        to_type: AssetType,
        value: Balance,
    ) -> StdResult<(), AssetErr>;
    fn on_issue(token: &Token, who: &AccountId, value: Balance) -> Result;
    fn on_destroy(token: &Token, who: &AccountId, value: Balance) -> Result;
    fn on_set_balance(
        _token: &Token,
        _who: &AccountId,
        _type: AssetType,
        _value: Balance,
    ) -> Result {
        Ok(())
    }
}

impl<AccountId, Balance> OnAssetChanged<AccountId, Balance> for () {
    fn on_move(
        _token: &Token,
        _from: &AccountId,
        _from_type: AssetType,
        _to: &AccountId,
        _to_type: AssetType,
        _value: Balance,
    ) -> StdResult<(), AssetErr> {
        Ok(())
    }
    fn on_issue(_: &Token, _: &AccountId, _: Balance) -> Result {
        Ok(())
    }
    fn on_destroy(_: &Token, _: &AccountId, _: Balance) -> Result {
        Ok(())
    }
}

pub trait OnAssetRegisterOrRevoke {
    fn on_register(_: &Token, _: bool) -> Result;
    fn on_revoke(_: &Token) -> Result;
}

impl OnAssetRegisterOrRevoke for () {
    fn on_register(_: &Token, _: bool) -> Result {
        Ok(())
    }
    fn on_revoke(_: &Token) -> Result {
        Ok(())
    }
}

impl<A: OnAssetRegisterOrRevoke, B: OnAssetRegisterOrRevoke> OnAssetRegisterOrRevoke for (A, B) {
    fn on_register(token: &Token, is_psedu_intention: bool) -> Result {
        let r = A::on_register(token, is_psedu_intention);
        let r2 = B::on_register(token, is_psedu_intention);
        if r.is_ok() == false {
            return r;
        } else if r2.is_ok() == false {
            return r2;
        }
        Ok(())
    }

    fn on_revoke(token: &Token) -> Result {
        let r = A::on_revoke(token);
        let r2 = B::on_revoke(token);
        if r.is_ok() == false {
            return r;
        } else if r2.is_ok() == false {
            return r2;
        }
        Ok(())
    }
}

struct AssetTriggerEventAfter<T: Trait>(::rstd::marker::PhantomData<T>);

impl<T: Trait> AssetTriggerEventAfter<T> {
    fn on_move(
        token: &Token,
        from: &T::AccountId,
        from_type: AssetType,
        to: &T::AccountId,
        to_type: AssetType,
        value: T::Balance,
    ) -> StdResult<(), AssetErr> {
        T::OnAssetChanged::on_move(token, from, from_type, to, to_type, value)?;
        Module::<T>::deposit_event(RawEvent::Move(
            token.clone(),
            from.clone(),
            from_type,
            to.clone(),
            to_type,
            value,
        ));
        Ok(())
    }
    fn on_issue(token: &Token, who: &T::AccountId, value: T::Balance) -> Result {
        T::OnAssetChanged::on_issue(token, who, value)?;
        Module::<T>::deposit_event(RawEvent::Issue(token.clone(), who.clone(), value));
        Ok(())
    }
    fn on_destroy(token: &Token, who: &T::AccountId, value: T::Balance) -> Result {
        T::OnAssetChanged::on_destroy(token, who, value)?;
        Module::<T>::deposit_event(RawEvent::Destory(token.clone(), who.clone(), value));
        Ok(())
    }
    fn on_set_balance(
        token: &Token,
        who: &T::AccountId,
        type_: AssetType,
        value: T::Balance,
    ) -> Result {
        T::OnAssetChanged::on_set_balance(token, who, type_, value)?;
        Module::<T>::deposit_event(RawEvent::Set(token.clone(), who.clone(), type_, value));
        Ok(())
    }
}

#[derive(PartialEq, PartialOrd, Ord, Eq, Clone, Copy, Encode, Decode)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize, Debug))]
pub enum AssetType {
    Free,
    ReservedStaking,
    ReservedStakingRevocation,
    ReservedWithdrawal,
    ReservedDexSpot,
    ReservedDexFuture,
}

// TODO use marco to improve it
impl AssetType {
    pub fn iterator() -> Iter<'static, AssetType> {
        static TYPES: [AssetType; 6] = [
            AssetType::Free,
            AssetType::ReservedStaking,
            AssetType::ReservedStakingRevocation,
            AssetType::ReservedWithdrawal,
            AssetType::ReservedDexSpot,
            AssetType::ReservedDexFuture,
        ];
        TYPES.iter()
    }
}

impl Default for AssetType {
    fn default() -> Self {
        AssetType::Free
    }
}

decl_event!(
    pub enum Event<T> where
        <T as system::Trait>::AccountId,
        <T as balances::Trait>::Balance
    {
        Move(Token, AccountId, AssetType, AccountId, AssetType, Balance),
        Issue(Token, AccountId, Balance),
        Destory(Token, AccountId, Balance),
        Set(Token, AccountId, AssetType, Balance),
        Register(Token, bool),
        Revoke(Token),
    }
);

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        fn deposit_event<T>() = default;

        /// register_asset to module, should allow by root
        fn register_asset(asset: Asset, is_online: bool, is_psedu_intention: bool, free: T::Balance) -> Result {
            asset.is_valid()?;

            let token = asset.token();

            Self::add_asset(asset, free)?;

            T::OnAssetRegisterOrRevoke::on_register(&token, is_psedu_intention)?;
            Self::deposit_event(RawEvent::Register(token.clone(), is_psedu_intention));

            if !is_online {
                let _ = Self::revoke_asset(token);
            }
            Ok(())
        }

        /// revoke asset, mark this asset is invalid
        fn revoke_asset(token: Token) -> Result {
            is_valid_token(&token)?;
            Self::remove_asset(&token)?;

            T::OnAssetRegisterOrRevoke::on_revoke(&token)?;
            Self::deposit_event(RawEvent::Revoke(token));
            Ok(())
        }

        /// set free token for an account
        fn set_balance(who: <T::Lookup as StaticLookup>::Source, token: Token, balances: CodecBTreeMap<AssetType, T::Balance>) -> Result {
            let who = <T as system::Trait>::Lookup::lookup(who)?;
            Self::set_balance_by_root(&who, &token, balances)?;
            Ok(())
        }

        /// transfer between account
        fn transfer(origin, dest: <T::Lookup as StaticLookup>::Source, token: Token, value: T::Balance, memo: Memo) -> Result {
            runtime_io::print("[xassets] transfer");
            let transactor = ensure_signed(origin)?;
            let dest = <T as system::Trait>::Lookup::lookup(dest)?;

            is_valid_memo::<T>(&memo)?;
            if transactor == dest {
                return Ok(())
            }
            Self::move_free_balance(&token, &transactor, &dest, value).map_err(|e| e.info())
        }
    }
}

decl_storage! {
    trait Store for Module<T: Trait> as XAssets {
        /// Asset token index list for Chain, different Chain has different token list
        pub AssetList get(asset_list): map Chain => Vec<Token>;

        /// asset info for every token, key is token token
        pub AssetInfo get(asset_info): map Token => Option<(Asset, bool, T::BlockNumber)>;

        /// asset balance for user&token, use btree_map to accept different asset type
        pub AssetBalance: map (T::AccountId, Token) => CodecBTreeMap<AssetType, T::Balance>;
        /// asset balance for a token, use btree_map to accept different asset type
        pub TotalAssetBalance: map Token => CodecBTreeMap<AssetType, T::Balance>;

        /// memo len
        pub MemoLen get(memo_len) config(): u32;
    }

    add_extra_genesis {
        config(asset_list): Vec<(Asset, bool, bool, Vec<(T::AccountId, u64)>)>;
        config(pcx): (Token, Precision, Desc);

        build(|storage: &mut primitives::StorageOverlay, _: &mut primitives::ChildrenStorageOverlay, config: &GenesisConfig<T>| {
            use runtime_io::with_externalities;
            use substrate_primitives::Blake2Hasher;
            use primitives::traits::{Zero, As};

            let src_r = storage.clone().build_storage().unwrap().0;
            let mut tmp_storage: runtime_io::TestExternalities<Blake2Hasher> = src_r.into();
            with_externalities(&mut tmp_storage, || {
                let chainx: Token = <Module<T> as ChainT>::TOKEN.to_vec();

                let pcx = Asset::new(chainx, config.pcx.0.clone(), Chain::ChainX, config.pcx.1, config.pcx.2.clone()).unwrap();
                Module::<T>::register_asset(pcx, true, false, Zero::zero()).unwrap();

                // init for asset_list
                for (asset, is_online, is_psedu_intention, init_list) in config.asset_list.iter() {
                    let token = asset.token();
                    Module::<T>::register_asset(asset.clone(), *is_online, *is_psedu_intention, Zero::zero()).unwrap();

                    for (accountid, value) in init_list {
                        let value = As::sa(*value);
                        let total_free_token = Module::<T>::total_asset_balance(&token, AssetType::Free);
                        let free_token = Module::<T>::free_balance(&accountid, &token);
                        Module::<T>::set_total_asset_balance(&token, AssetType::Free, total_free_token + value);
                        // not create account
                        Module::<T>::set_asset_balance(&accountid, &token, AssetType::Free, free_token + value);
                    }
                }

            });
            let map: primitives::StorageOverlay = tmp_storage.into();
            storage.extend(map);
        });
    }
}

impl<T: Trait> ChainT for Module<T> {
    const TOKEN: &'static [u8] = b"PCX";
    fn chain() -> Chain {
        Chain::ChainX
    }
}

impl<T: Trait> Module<T> {
    // token storage
    pub fn asset_balance(who: &T::AccountId, token: &Token, type_: AssetType) -> T::Balance {
        if token.as_slice() == <Self as ChainT>::TOKEN && type_ == AssetType::Free {
            balances::Module::<T>::free_balance(who)
        } else {
            *AssetBalance::<T>::get(&(who.clone(), token.clone()))
                .0
                .get(&type_)
                .unwrap_or(&Zero::zero())
        }
    }

    fn set_asset_balance(who: &T::AccountId, token: &Token, type_: AssetType, val: T::Balance) {
        if token.as_slice() == <Self as ChainT>::TOKEN && type_ == AssetType::Free {
            balances::Module::<T>::set_free_balance(who, val);
        } else {
            AssetBalance::<T>::mutate(&(who.clone(), token.clone()), |m| {
                let _ = m.0.insert(type_, val); // update the value
            });
        }
    }

    /// free balance for a account for a token
    pub fn free_balance(who: &T::AccountId, token: &Token) -> T::Balance {
        Self::asset_balance(who, token, AssetType::Free)
    }

    fn set_free_balance_creating(who: &T::AccountId, token: &Token, value: T::Balance) {
        let is_existing = balances::FreeBalance::<T>::exists(who);
        if token.as_slice() == <Self as ChainT>::TOKEN {
            if is_existing {
                balances::Module::<T>::set_free_balance(who, value);
            } else {
                // set_free_balance_creating would access `existential_deposit` storage
                balances::Module::<T>::set_free_balance_creating(who, value);
            }
        } else {
            if is_existing == false {
                balances::Module::<T>::set_free_balance_creating(who, Zero::zero());
            }
            Self::set_asset_balance(who, token, AssetType::Free, value)
        }
    }

    pub fn total_asset_balance(token: &Token, type_: AssetType) -> T::Balance {
        if token.as_slice() == <Self as ChainT>::TOKEN && type_ == AssetType::Free {
            let other_types = TotalAssetBalance::<T>::get(token)
                .0
                .iter()
                .filter(|(&k, _)| k != AssetType::Free) // remove free calc
                .fold(Zero::zero(), |acc, (_, v)| acc + *v);
            balances::TotalIssuance::<T>::get() - other_types
        } else {
            *TotalAssetBalance::<T>::get(token)
                .0
                .get(&type_)
                .unwrap_or(&Zero::zero())
        }
    }

    fn set_total_asset_balance(token: &Token, type_: AssetType, value: T::Balance) {
        if token.as_slice() == <Self as ChainT>::TOKEN && type_ == AssetType::Free {
            // do nothing
        } else {
            TotalAssetBalance::<T>::mutate(token, |m| {
                let _ = m.0.insert(type_, value); // update the value
            });
        }
    }

    /// all type balance of `who` for token
    pub fn all_type_balance_of(who: &T::AccountId, token: &Token) -> T::Balance {
        let map = Self::balance_of(who, token);
        map.0.iter().fold(Zero::zero(), |acc, (_, v)| acc + *v)
    }

    pub fn balance_of(who: &T::AccountId, token: &Token) -> CodecBTreeMap<AssetType, T::Balance> {
        let mut bmap = BTreeMap::<AssetType, T::Balance>::from_iter(
            AssetType::iterator().map(|t| (*t, Zero::zero())),
        );

        let result = AssetBalance::<T>::get(&(who.clone(), token.clone()));
        bmap.extend(result.0.iter());

        // PCX free balance
        if token.as_slice() == Self::TOKEN {
            let free = balances::Module::<T>::free_balance(who);
            bmap.insert(AssetType::Free, free);
        }
        CodecBTreeMap(bmap)
    }

    /// all type balance of a token
    pub fn all_type_balance(token: &Token) -> T::Balance {
        if token.as_slice() == <Self as ChainT>::TOKEN {
            balances::TotalIssuance::<T>::get()
        } else {
            TotalAssetBalance::<T>::get(token)
                .0
                .iter()
                .fold(Zero::zero(), |acc, (_, v)| acc + *v)
        }
    }

    pub fn should_not_free_type(type_: AssetType) -> Result {
        if type_ == AssetType::Free {
            return Err("should not be free type here");
        }
        Ok(())
    }

    pub fn should_not_chainx(token: &Token) -> Result {
        if token.as_slice() == <Self as ChainT>::TOKEN {
            return Err("should not use chainx token here");
        }
        Ok(())
    }
}

// asset related
impl<T: Trait> Module<T> {
    /// add an asset into the storage, notice the asset must be valid
    fn add_asset(asset: Asset, free: T::Balance) -> Result {
        let token = asset.token();
        let chain = asset.chain();
        if AssetInfo::<T>::exists(&token) {
            return Err("already has this token");
        }

        AssetInfo::<T>::insert(&token, (asset, true, system::Module::<T>::block_number()));

        AssetList::<T>::mutate(chain, |v| {
            v.push(token.clone());
        });

        Self::init_asset_balance(&token, free);
        Ok(())
    }

    fn init_asset_balance(token: &Token, free: T::Balance) {
        Self::set_total_asset_balance(token, AssetType::Free, free);
    }

    fn remove_asset(token: &Token) -> Result {
        if let Some(mut info) = AssetInfo::<T>::get(token) {
            // let chain = info.0.chain();
            info.1 = false;
            AssetInfo::<T>::insert(token.clone(), info);
            // remove this token index from AssetList
            // AssetList::<T>::mutate(chain, |v| {
            //     v.retain(|i| i != token);
            // });

            Ok(())
        } else {
            Err("this token dose not register yet or is invalid")
        }
    }

    pub fn is_valid_asset(token: &Token) -> Result {
        is_valid_token(token)?;

        if let Some(info) = Self::asset_info(token) {
            if info.1 == true {
                return Ok(());
            }
            return Err("not a valid token");
        }
        Err("not a registered token")
    }

    pub fn assets() -> Vec<Token> {
        let mut v = Vec::new();
        for i in Chain::iterator() {
            v.extend(Self::asset_list(i));
        }
        v
    }

    pub fn all_assets() -> Vec<(Asset, bool)> {
        let list = Self::assets();
        let mut v = Vec::new();
        for token in list {
            if let Some((asset, valid, _)) = Self::asset_info(token) {
                v.push((asset, valid))
            }
        }
        v
    }

    /// notice don't call this func in runtime
    pub fn valid_assets() -> Vec<Token> {
        Self::assets()
            .into_iter()
            .filter(|t| {
                if let Some(t) = Self::asset_info(t) {
                    t.1
                } else {
                    false
                }
            })
            .collect()
    }

    pub fn valid_assets_of(
        who: &T::AccountId,
    ) -> Vec<(Token, CodecBTreeMap<AssetType, T::Balance>)> {
        let tokens = Self::valid_assets();
        let mut list = Vec::new();
        for token in tokens.into_iter() {
            let has_asset = if token.as_slice() == <Self as ChainT>::TOKEN {
                balances::FreeBalance::<T>::exists(who)
            } else {
                AssetBalance::<T>::exists(&(who.clone(), token.clone()))
            };

            if has_asset {
                let map = Self::balance_of(who, &token);
                list.push((token, map));
            }
        }
        list
    }

    pub fn get_asset(token: &Token) -> StdResult<Asset, &'static str> {
        if let Some((asset, valid, _)) = Self::asset_info(token) {
            if valid == false {
                return Err("this asset is invalid, maybe has been revoked.");
            }
            Ok(asset)
        } else {
            return Err("this token asset not exist!");
        }
    }
}

/// token issue destroy reserve/unreserve
impl<T: Trait> Module<T> {
    pub fn issue(token: &Token, who: &T::AccountId, value: T::Balance) -> Result {
        if token.as_slice() == Self::TOKEN {
            match Self::pcx_free_balance(who).checked_add(&value) {
                Some(b) => Self::pcx_set_free_balance_creating(who, b),
                None => return Err("free balance too high to issue"),
            };
            Self::increase_total_stake_by(value);

            AssetTriggerEventAfter::<T>::on_issue(token, who, value)?;
            return Ok(());
        }

        // Self::should_not_chainx(token)?;
        Self::is_valid_asset(token)?;

        let total_free_token = Self::total_asset_balance(token, AssetType::Free);
        let free_token = Self::asset_balance(who, token, AssetType::Free);
        // check
        let new_free_token = match free_token.checked_add(&value) {
            Some(b) => b,
            None => return Err("free balance too high to issue"),
        };
        let new_total_free_token = match total_free_token.checked_add(&value) {
            Some(b) => b,
            None => return Err("total free balance too high to issue"),
        };

        Self::set_total_asset_balance(token, AssetType::Free, new_total_free_token);
        //        Self::set_asset_balance(who, token, AssetType::Free, new_free_token);
        Self::set_free_balance_creating(who, token, new_free_token);

        AssetTriggerEventAfter::<T>::on_issue(token, who, value)?;
        Ok(())
    }

    pub fn destroy(token: &Token, who: &T::AccountId, value: T::Balance) -> Result {
        Self::should_not_chainx(token)?;
        Self::is_valid_asset(token)?;
        let type_ = AssetType::ReservedWithdrawal;

        // get storage
        let total_reserved_token = Self::total_asset_balance(token, type_);
        let reserved_token = Self::asset_balance(who, token, type_);
        // check
        let new_reserved_token = match reserved_token.checked_sub(&value) {
            Some(b) => b,
            None => return Err("reserved balance too low to destroy"),
        };
        let new_total_reserved_token = match total_reserved_token.checked_sub(&value) {
            Some(b) => b,
            None => return Err("total reserved balance too low to destroy"),
        };

        // set to storage
        Self::set_total_asset_balance(token, type_, new_total_reserved_token);
        Self::set_asset_balance(who, token, type_, new_reserved_token);

        AssetTriggerEventAfter::<T>::on_destroy(token, who, value)?;
        Ok(())
    }

    pub fn move_balance(
        token: &Token,
        from: &T::AccountId,
        from_type: AssetType,
        to: &T::AccountId,
        to_type: AssetType,
        value: T::Balance,
    ) -> StdResult<(), AssetErr> {
        Self::move_balance_with_checkflag(token, from, from_type, to, to_type, value, true)
    }

    pub fn move_balance_with_checkflag(
        token: &Token,
        from: &T::AccountId,
        from_type: AssetType,
        to: &T::AccountId,
        to_type: AssetType,
        value: T::Balance,
        check: bool,
    ) -> StdResult<(), AssetErr> {
        if from == to && from_type == to_type {
            // same account, same type, return directly
            return Ok(());
        }
        if value == Zero::zero() {
            return Ok(());
        }

        if check {
            Self::is_valid_asset(token).map_err(|_| AssetErr::InvalidToken)?;
        }

        let from_balance = Self::asset_balance(from, token, from_type);
        let to_balance = Self::asset_balance(to, token, to_type);

        // test overflow
        let new_from_balance = match from_balance.checked_sub(&value) {
            Some(b) => b,
            None => return Err(AssetErr::NotEnough),
        };
        let new_to_balance = match to_balance.checked_add(&value) {
            Some(b) => b,
            None => return Err(AssetErr::OverFlow),
        };

        // for total
        if from_type != to_type {
            let total_from_balance = Self::total_asset_balance(token, from_type);
            let total_to_balance = Self::total_asset_balance(token, to_type);

            let new_total_from_balance = match total_from_balance.checked_sub(&value) {
                Some(b) => b,
                None => return Err(AssetErr::TotalAssetNotEnough),
            };

            let new_total_to_balance = match total_to_balance.checked_add(&value) {
                Some(b) => b,
                None => return Err(AssetErr::TotalAssetOverFlow),
            };
            // for asset to set storage
            Self::set_total_asset_balance(token, from_type, new_total_from_balance);
            Self::set_total_asset_balance(token, to_type, new_total_to_balance);
        }
        // for account to set storage
        Self::set_asset_balance(from, token, from_type, new_from_balance);
        if to_type == AssetType::Free {
            Self::set_free_balance_creating(to, token, new_to_balance);
        } else {
            Self::set_asset_balance(to, token, to_type, new_to_balance);
        }

        AssetTriggerEventAfter::<T>::on_move(token, from, from_type, to, to_type, value)?;
        Ok(())
    }

    pub fn move_free_balance(
        token: &Token,
        from: &T::AccountId,
        to: &T::AccountId,
        value: T::Balance,
    ) -> StdResult<(), AssetErr> {
        Self::move_balance(token, from, AssetType::Free, to, AssetType::Free, value)
    }

    pub fn set_balance_by_root(
        who: &T::AccountId,
        token: &Token,
        balances: CodecBTreeMap<AssetType, T::Balance>,
    ) -> Result {
        for (type_, val) in balances.0.into_iter() {
            let old_val = Self::asset_balance(who, token, type_);
            let old_total_val = Self::total_asset_balance(token, type_);
            if old_val == val {
                continue;
            }

            let new_total_val = if val > old_val {
                match val.checked_sub(&old_val) {
                    None => return Err("balance too low to sub value"),
                    Some(b) => match old_total_val.checked_add(&b) {
                        None => return Err("old total balance too high to add value"),
                        Some(new) => new,
                    },
                }
            } else {
                match old_val.checked_sub(&val) {
                    None => return Err("old balance too low to sub value"),
                    Some(b) => match old_total_val.checked_sub(&b) {
                        None => return Err("old total balance too low to sub value"),
                        Some(new) => new,
                    },
                }
            };

            Self::set_asset_balance(who, token, type_, val);
            if token.as_slice() == <Self as ChainT>::TOKEN && type_ == AssetType::Free {
                balances::TotalIssuance::<T>::put(new_total_val)
            } else {
                Self::set_total_asset_balance(token, type_, new_total_val);
            }
            AssetTriggerEventAfter::<T>::on_set_balance(token, who, type_, val)?;
        }
        Ok(())
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Encode, Decode)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize, Debug))]
pub enum AssetErr {
    NotEnough,
    OverFlow,
    TotalAssetOverFlow,
    TotalAssetNotEnough,
    InvalidToken,
    InvalidAccount,
}

impl AssetErr {
    pub fn info(self) -> &'static str {
        match self {
            AssetErr::NotEnough => "balance too low for this account",
            AssetErr::OverFlow => "balance too high for this account",
            AssetErr::TotalAssetOverFlow => "balance too low for this asset",
            AssetErr::TotalAssetNotEnough => "balance too high for this asset",
            AssetErr::InvalidToken => "not a valid token for this account",
            AssetErr::InvalidAccount => "account Locked",
        }
    }
}

// wrapper for balances module
impl<T: Trait> Module<T> {
    pub fn pcx_free_balance(who: &T::AccountId) -> T::Balance {
        Self::free_balance(who, &<Self as ChainT>::TOKEN.to_vec())
    }

    pub fn pcx_total_balance(who: &T::AccountId) -> T::Balance {
        Self::all_type_balance_of(who, &<Self as ChainT>::TOKEN.to_vec())
    }

    fn pcx_set_free_balance_creating(who: &T::AccountId, value: T::Balance) {
        Self::set_free_balance_creating(who, &<Self as ChainT>::TOKEN.to_vec(), value);
    }

    pub fn pcx_issue(who: &T::AccountId, value: T::Balance) -> Result {
        Self::issue(&Self::TOKEN.to_vec(), who, value)
    }

    pub fn pcx_move_balance(
        from: &T::AccountId,
        from_type: AssetType,
        to: &T::AccountId,
        to_type: AssetType,
        value: T::Balance,
    ) -> StdResult<(), AssetErr> {
        Self::move_balance(
            &<Self as ChainT>::TOKEN.to_vec(),
            from,
            from_type,
            to,
            to_type,
            value,
        )
    }

    pub fn pcx_move_free_balance(
        from: &T::AccountId,
        to: &T::AccountId,
        value: T::Balance,
    ) -> StdResult<(), AssetErr> {
        Self::move_balance(
            &<Self as ChainT>::TOKEN.to_vec(),
            from,
            AssetType::Free,
            to,
            AssetType::Free,
            value,
        )
    }

    pub fn increase_total_stake_by(value: T::Balance) {
        balances::Module::<T>::increase_total_stake_by(value);
    }
}
