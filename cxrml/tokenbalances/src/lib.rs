// Copyright 2018 Chainpool.
//! TokenBalances: Handles token symbol balances.

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

// for encode/decode
//#[cfg(feature = "std")]
//extern crate serde;
#[cfg(feature = "std")]
#[macro_use]
extern crate serde_derive;
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
extern crate srml_system as system;
extern crate srml_balances as balances;

// for chainx runtime module lib
extern crate cxrml_support as cxsupport;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

use rstd::prelude::*;
pub use rstd::result::Result as StdResult;
use codec::Codec;
use runtime_support::{StorageValue, StorageMap, Parameter};
use runtime_support::dispatch::Result;
use primitives::traits::{SimpleArithmetic, As, Member, CheckedAdd, CheckedSub, OnFinalise};

// substrate mod
use system::ensure_signed;
use balances::address::Address;
use balances::EnsureAccountLiquid;

pub type SymbolString = &'static [u8];

pub type DescString = SymbolString;

pub trait Trait: balances::Trait + cxsupport::Trait {
    const CHAINX_SYMBOL: SymbolString;
    const CHAINX_PRECISION: Precision;
    const CHAINX_TOKEN_DESC: DescString;
    /// The token balance.
    type TokenBalance: Parameter + Member + Codec + SimpleArithmetic + As<u64> + As<u128> + Copy + Default;
    /// Event
    type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

pub type Symbol = Vec<u8>;
pub type TokenDesc = Vec<u8>;
pub type Precision = u16;

const MAX_SYMBOL_LEN: usize = 32;
const MAX_TOKENDESC_LEN: usize = 128;

pub fn is_valid_symbol(v: &[u8]) -> Result {
    if v.len() > MAX_SYMBOL_LEN || v.len() == 0 {
        Err("symbol length too long or zero")
    } else {
        for c in v.iter() {
            // allow number (0x30~0x39), capital letter (0x41~0x5A), small letter (0x61~0x7A), - 0x2D, . 0x2E, | 0x7C,  ~ 0x7E
            if (*c >= 0x30 && *c <= 0x39) // number
                || (*c >= 0x41 && *c <= 0x5A) // capital
                || (*c >= 0x61 && *c <= 0x7A) // small
                || (*c == 0x2D) // -
                || (*c == 0x2E) // .
                || (*c == 0x7C) // |
                || (*c == 0x7E) // ~
                { continue; } else {
                return Err("not a valid symbol char for number, capital/small letter or '-', '.', '|', '~'");
            }
        }
        Ok(())
    }
}

pub fn is_valid_token_desc(v: &[u8]) -> Result {
    if v.len() > MAX_TOKENDESC_LEN { Err("token desc length too long") } else {
        for c in v.iter() {
            // ascii visible char
            if *c >= 20 && *c <= 0x7E
                { continue; } else {
                return Err("not a valid ascii visible char");
            }
        }
        Ok(())
    }
}

/// Token struct.
#[derive(PartialEq, Eq, Clone, Encode, Decode, Default)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize, Debug))]
pub struct Token {
    /// Validator should ensure this many more slashes than is necessary before being unstaked.
    symbol: Symbol,
    /// token description
    token_desc: TokenDesc,
    /// token balance precision
    precision: Precision,
}

impl Token {
    pub fn new(symbol: Symbol, token_desc: TokenDesc, precision: Precision) -> Self {
        Token { symbol, token_desc, precision }
    }

    pub fn symbol(&self) -> Symbol {
        self.symbol.clone()
    }

    pub fn precision(&self) -> Precision {
        self.precision
    }

    pub fn token_desc(&self) -> TokenDesc {
        self.token_desc.clone()
    }

    pub fn set_token_desc(&mut self, desc: &TokenDesc) {
        self.token_desc = desc.clone();
    }

    pub fn is_valid(&self) -> Result {
        is_valid_symbol(&self.symbol)?;
        is_valid_token_desc(&self.token_desc)?;
        Ok(())
    }
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        /// register_token to module, should allow by root
        fn register_token(token: Token, free: T::TokenBalance, reversed: T::TokenBalance) -> Result;
        /// transfer between account
        fn transfer_token(origin, dest: Address<T::AccountId, T::AccountIndex>, sym: Symbol, value: T::TokenBalance) -> Result;
        // set transfer token fee
        fn set_transfer_token_fee(val: T::Balance) -> Result;
    }
}

decl_event!(
    pub enum Event<T> where
        <T as system::Trait>::AccountId,
        <T as Trait>::TokenBalance,
        <T as balances::Trait>::Balance
    {
        /// register new token (token.symbol(), token.token_desc, token.precision)
        RegisterToken(Symbol, TokenDesc, Precision),
        /// cancel token
        CancelToken(Symbol),
        /// issue succeeded (who, symbol, balance)
        IssueToken(AccountId, Symbol, TokenBalance),
        // TODO
        /// lock destroy (who, symbol, balance)
        ReverseToken(AccountId, Symbol, TokenBalance),
        /// unlock destroy (who, symbol, balance)
        UnreverseToken(AccountId, Symbol, TokenBalance),
        /// destroy
        DestroyToken(AccountId, Symbol, TokenBalance),
        /// Transfer succeeded (from, to, symbol, value, fees).
        TransferToken(AccountId, AccountId, Symbol, TokenBalance, Balance),
        /// Move Free Token, include chainx (from, to, symbol, value)
        MoveFreeToken(AccountId, AccountId, Symbol, TokenBalance),
        /// set transfer token fee
        SetTransferTokenFee(Balance),
    }
);

decl_storage! {
    trait Store for Module<T: Trait> as TokenBalances {
        /// supported token list
        pub TokenListMap get(token_list_map): map u32 => Symbol;
        /// supported token list length
        pub TokenListLen get(token_list_len): u32;
        /// token info for every token, key is token symbol
        pub TokenInfo get(token_info): map Symbol => Option<(Token, bool)>;

        /// total free token of a symbol
        pub TotalFreeToken get(total_free_token): map Symbol => T::TokenBalance;

        pub FreeToken: map (T::AccountId, Symbol) => T::TokenBalance;

        /// total locked token of a symbol
        pub TotalReservedToken get(total_reserved_token): map Symbol => T::TokenBalance;

        pub ReservedToken get(reserved_token): map (T::AccountId, Symbol) => T::TokenBalance;

        /// token list of a account
        pub TokenListOf get(token_list_of): map T::AccountId => Vec<Symbol> = [T::CHAINX_SYMBOL.to_vec()].to_vec();

        /// transfer token fee
        pub TransferTokenFee get(transfer_token_fee) config(): T::Balance;
    }
    add_extra_genesis {
        config(token_list): Vec<(Token, T::TokenBalance, T::TokenBalance)>;
        build(
            |storage: &mut primitives::StorageMap, config: &GenesisConfig<T>| {
                use codec::Encode;
                let mut list_count = 0_u32;
                // insert chainx token symbol
                let chainx: Symbol = T::CHAINX_SYMBOL.to_vec();
                storage.insert(GenesisConfig::<T>::hash(&<TokenListMap<T>>::key_for(&list_count)).to_vec(), chainx.clone().encode());
                // token info
                let t: Token = Token::new(chainx.clone(), T::CHAINX_TOKEN_DESC.to_vec(), T::CHAINX_PRECISION);
                storage.insert(GenesisConfig::<T>::hash(&<TokenInfo<T>>::key_for(&chainx)).to_vec(), (t, true).encode());
                list_count += 1;

                // 0 token list length
                storage.insert(GenesisConfig::<T>::hash(&<TokenListLen<T>>::key()).to_vec(), (config.token_list.len() as u32 + list_count).encode());
                for (index, (token, free_token, reserved_token)) in config.token_list.iter().enumerate() {
                    if let Err(e) = token.is_valid() {
                        panic!(e);
                    }
                    // 1 token balance
                    storage.insert(GenesisConfig::<T>::hash(&<TotalFreeToken<T>>::key_for(token.symbol())).to_vec(), free_token.encode());
                    storage.insert(GenesisConfig::<T>::hash(&<TotalReservedToken<T>>::key_for(token.symbol())).to_vec(), reserved_token.encode());
                    // 2 token list map
                    storage.insert(GenesisConfig::<T>::hash(&<TokenListMap<T>>::key_for(index as u32 + list_count)).to_vec(), token.symbol().encode());
                    // 3 token info
                    storage.insert(GenesisConfig::<T>::hash(&<TokenInfo<T>>::key_for(token.symbol())).to_vec(), (token, true).encode());
                }
            }
        );
    }
}

// This trait expresses what should happen when the block is finalised.
impl<T: Trait> OnFinalise<T::BlockNumber> for Module<T> {
    fn on_finalise(_: T::BlockNumber) {
        // do nothing
    }
}

impl<T: Trait> Module<T> {
    /// Deposit one of this module's events.
    fn deposit_event(event: Event<T>) {
        <system::Module<T>>::deposit_event(<T as Trait>::Event::from(event).into());
    }
}

impl<T: Trait> Module<T> {
    // token storage
    pub fn free_token(who_sym: &(T::AccountId, Symbol)) -> T::TokenBalance {
        if who_sym.1.as_slice() == T::CHAINX_SYMBOL {
            As::sa(balances::FreeBalance::<T>::get(&who_sym.0).as_())
        } else {
            <FreeToken<T>>::get(who_sym)
        }
    }

    /// The combined token balance of `who` for symbol.
    pub fn total_token_of(who: &T::AccountId, symbol: &Symbol) -> T::TokenBalance {
        Self::free_token(&(who.clone(), symbol.clone())) + Self::reserved_token((who.clone(), symbol.clone()))
    }

    /// tatal_token of a token symbol
    pub fn total_token(symbol: &Symbol) -> T::TokenBalance {
        if symbol.as_slice() == T::CHAINX_SYMBOL {
            As::sa(balances::TotalIssuance::<T>::get().as_())
        } else {
            Self::total_free_token(symbol) + Self::total_reserved_token(symbol)
        }
    }
}

impl<T: Trait> Module<T> {
    // token symol
    // public call
    /// register a token into token list ans init
    pub fn register_token(token: Token, free: T::TokenBalance, reserved: T::TokenBalance) -> Result {
        token.is_valid()?;
        let sym = token.symbol();
        Self::add_token(&sym, free, reserved)?;
        <TokenInfo<T>>::insert(&sym, (token.clone(), true));

        Self::deposit_event(RawEvent::RegisterToken(token.symbol(), token.token_desc(), token.precision()));
        Ok(())
    }
    /// cancel a token from token list but not remove it
    pub fn cancel_token(symbol: &Symbol) -> Result {
        is_valid_symbol(symbol)?;
        Self::remove_token(symbol)?;

        Self::deposit_event(RawEvent::CancelToken(symbol.clone()));
        Ok(())
    }

    pub fn token_list() -> Vec<Symbol> {
        let len: u32 = <TokenListLen<T>>::get();
        let mut v: Vec<Symbol> = Vec::new();
        for i in 0..len {
            let symbol = <TokenListMap<T>>::get(i);
            v.push(symbol);
        }
        v
    }

    pub fn valid_token_list() -> Vec<Symbol> {
        Self::token_list().into_iter()
            .filter(|s| {
                if let Some(t) = TokenInfo::<T>::get(s) {
                    t.1
                } else { false }
            })
            .collect()
    }

    pub fn is_valid_token(symbol: &Symbol) -> Result {
        is_valid_symbol(symbol)?;
        if let Some(info) = TokenInfo::<T>::get(symbol) {
            if info.1 == true {
                return Ok(());
            }
            return Err("not a valid token");
        }
        Err("not a registered token")
    }

    pub fn is_valid_token_for(who: &T::AccountId, symbol: &Symbol) -> Result {
        Self::is_valid_token(symbol)?;
        if Self::token_list_of(who).contains(symbol) {
            Ok(())
        } else {
            Err("not a existed token in this account token list")
        }
    }

    fn add_token(symbol: &Symbol, free: T::TokenBalance, reserved: T::TokenBalance) -> Result {
        if TokenInfo::<T>::exists(symbol) {
            return Err("already has this token symbol");
        }

        let len: u32 = <TokenListLen<T>>::get();
        // mark new symbol valid
        <TokenListMap<T>>::insert(len, symbol.clone());
        <TokenListLen<T>>::put(len + 1);

        Self::init_token_balance(symbol, free, reserved);

        Ok(())
    }

    fn remove_token(symbol: &Symbol) -> Result {
        is_valid_symbol(symbol)?;
        if let Some(mut info) = TokenInfo::<T>::get(symbol) {
            info.1 = false;
            TokenInfo::<T>::insert(symbol.clone(), info);
            Ok(())
        } else {
            Err("this token symbol dose not register yet or is invalid")
        }
    }

    fn init_token_balance(symbol: &Symbol, free: T::TokenBalance, reserved: T::TokenBalance) {
        <TotalFreeToken<T>>::insert(symbol, free);
        <TotalReservedToken<T>>::insert(symbol, reserved);
    }

    #[allow(unused)]
    fn remove_token_balance(symbol: &Symbol) {
        <TotalFreeToken<T>>::remove(symbol);
        <TotalReservedToken<T>>::remove(symbol);
    }
}

impl<T: Trait> Module<T> {
    fn init_token_for(who: &T::AccountId, symbol: &Symbol) {
        if let Err(_) = Self::is_valid_token_for(who, symbol) {
            <TokenListOf<T>>::mutate(who, |token_list| token_list.push(symbol.clone()));
        }
    }

    /// issue from real coin to chainx token, notice it become free token directly
    pub fn issue(who: &T::AccountId, symbol: &Symbol, value: T::TokenBalance) -> Result {
        if symbol.as_slice() == T::CHAINX_SYMBOL {
            return Err("can't issue chainx token");
        }

        Self::is_valid_token(symbol)?;

        <T as balances::Trait>::EnsureAccountLiquid::ensure_account_liquid(who)?;

        // get storage
        let key = (who.clone(), symbol.clone());
        let total_free_token = TotalFreeToken::<T>::get(symbol);
        let free_token = FreeToken::<T>::get(&key);
        // check
        let new_free_token = match free_token.checked_add(&value) {
            Some(b) => b,
            None => return Err("free token too high to issue"),
        };
        let new_total_free_token = match total_free_token.checked_add(&value) {
            Some(b) => b,
            None => return Err("total free token too high to issue"),
        };
        // set to storage
        Self::init_token_for(who, symbol);
        TotalFreeToken::<T>::insert(symbol, new_total_free_token);
        FreeToken::<T>::insert(&key, new_free_token);

        Self::deposit_event(RawEvent::IssueToken(who.clone(), symbol.clone(), value));
        Ok(())
    }

    pub fn destroy(who: &T::AccountId, symbol: &Symbol, value: T::TokenBalance) -> Result {
        if symbol.as_slice() == T::CHAINX_SYMBOL {
            return Err("can't destroy chainx token");
        }
        Self::is_valid_token_for(who, symbol)?;
        <T as balances::Trait>::EnsureAccountLiquid::ensure_account_liquid(who)?;
        //TODO validator

        // get storage
        let key = (who.clone(), symbol.clone());
        let total_reserved_token = TotalReservedToken::<T>::get(symbol);
        let reserved_token = ReservedToken::<T>::get(&key);
        // check
        let new_reserved_token = match reserved_token.checked_sub(&value) {
            Some(b) => b,
            None => return Err("reserved token too low to destroy"),
        };
        let new_total_reserved_token = match total_reserved_token.checked_sub(&value) {
            Some(b) => b,
            None => return Err("total reserved token too low to destroy"),
        };
        // set to storage
        TotalReservedToken::<T>::insert(symbol, new_total_reserved_token);
        ReservedToken::<T>::insert(&key, new_reserved_token);

        Self::deposit_event(RawEvent::DestroyToken(who.clone(), symbol.clone(), value));
        Ok(())
    }

    pub fn reserve(who: &T::AccountId, symbol: &Symbol, value: T::TokenBalance) -> Result {
        Self::is_valid_token_for(who, symbol)?;
        <T as balances::Trait>::EnsureAccountLiquid::ensure_account_liquid(who)?;
        //TODO validator

        let key = (who.clone(), symbol.clone());
        // for chainx
        if symbol.as_slice() == T::CHAINX_SYMBOL {
            let value: T::Balance = As::sa(value.as_() as u64); // change to balance for balances module
            let free_token: T::Balance = balances::FreeBalance::<T>::get(who);
            let reserved_token = ReservedToken::<T>::get(&key);
            let total_reserved_token = TotalReservedToken::<T>::get(symbol);
            match free_token.checked_sub(&value) {
                Some(b) => b,
                None => return Err("chainx free token too low to reserve"),
            };
            let val: T::TokenBalance = As::sa(value.as_() as u128); // tokenbalance is large than balance
            let new_reserved_token = match reserved_token.checked_add(&val) {
                Some(b) => b,
                None => return Err("chainx reserved token too high to reserve"),
            };
            let new_total_reserved_token = match total_reserved_token.checked_add(&val) {
                Some(b) => b,
                None => return Err("chainx total reserved token too high to reserve"),
            };
            // would subtract freebalance and add to reversed balance
            balances::Module::<T>::reserve(who, value)?;
            ReservedToken::<T>::insert(key, new_reserved_token);
            TotalReservedToken::<T>::insert(symbol, new_total_reserved_token);

            Self::deposit_event(RawEvent::ReverseToken(who.clone(), T::CHAINX_SYMBOL.to_vec(), val));
            return Ok(());
        }

        // for other token
        // get from storage
        let total_free_token = TotalFreeToken::<T>::get(symbol);
        let total_reserved_token = TotalReservedToken::<T>::get(symbol);
        let free_token = FreeToken::<T>::get(&key);
        let reserved_token = ReservedToken::<T>::get(&key);
        // test overflow
        let new_free_token = match free_token.checked_sub(&value) {
            Some(b) => b,
            None => return Err("free token too low to reserve"),
        };
        let new_reserved_token = match reserved_token.checked_add(&value) {
            Some(b) => b,
            None => return Err("reserved token too high to reserve"),
        };
        let new_total_free_token = match total_free_token.checked_sub(&value) {
            Some(b) => b,
            None => return Err("total free token too low to reserve"),
        };
        let new_total_reserved_token = match total_reserved_token.checked_add(&value) {
            Some(b) => b,
            None => return Err("total reserved token too high to reserve"),
        };
        // set to storage
        TotalFreeToken::<T>::insert(symbol, new_total_free_token);
        TotalReservedToken::<T>::insert(symbol, new_total_reserved_token);
        FreeToken::<T>::insert(&key, new_free_token);
        ReservedToken::<T>::insert(&key, new_reserved_token);

        Self::deposit_event(RawEvent::ReverseToken(who.clone(), symbol.clone(), value));
        Ok(())
    }

    pub fn unreserve(who: &T::AccountId, symbol: &Symbol, value: T::TokenBalance) -> Result {
        Self::is_valid_token_for(who, symbol)?;
        <T as balances::Trait>::EnsureAccountLiquid::ensure_account_liquid(who)?;
        //TODO validator

        let key = (who.clone(), symbol.clone());
        // for chainx
        if symbol.as_slice() == T::CHAINX_SYMBOL {
            let value: T::Balance = As::sa(value.as_() as u64); // change to balance for balances module
            let free_token: T::Balance = balances::FreeBalance::<T>::get(who);
            let reserved_token = ReservedToken::<T>::get(&key);
            let total_reserved_token = TotalReservedToken::<T>::get(symbol);
            match free_token.checked_add(&value) {
                Some(b) => b,
                None => return Err("chainx free token too high to unreserve"),
            };
            let val: T::TokenBalance = As::sa(value.as_() as u128); // tokenbalance is large than balance
            let new_reserved_token = match reserved_token.checked_sub(&val) {
                Some(b) => b,
                None => return Err("chainx reserved token too low to unreserve"),
            };
            let new_total_reserved_token = match total_reserved_token.checked_sub(&val) {
                Some(b) => b,
                None => return Err("chainx total reserved token too low to unreserve"),
            };
            // would subtract reservedbalance and add to free balance
            balances::Module::<T>::unreserve(who, value);
            ReservedToken::<T>::insert(key, new_reserved_token);
            TotalReservedToken::<T>::insert(symbol, new_total_reserved_token);

            Self::deposit_event(RawEvent::UnreverseToken(who.clone(), T::CHAINX_SYMBOL.to_vec(), val));
            return Ok(());
        }

        // for other token
        // get from storage
        let total_free_token = TotalFreeToken::<T>::get(symbol);
        let total_reserved_token = TotalReservedToken::<T>::get(symbol);
        let free_token = FreeToken::<T>::get(&key);
        let reserved_token = ReservedToken::<T>::get(&key);
        // test overflow
        let new_free_token = match free_token.checked_add(&value) {
            Some(b) => b,
            None => return Err("free token too high to unreserve"),
        };
        let new_reserved_token = match reserved_token.checked_sub(&value) {
            Some(b) => b,
            None => return Err("reserved token too low to unreserve"),
        };
        let new_total_free_token = match total_free_token.checked_add(&value) {
            Some(b) => b,
            None => return Err("total free token too high to unreserve"),
        };
        let new_total_reserved_token = match total_reserved_token.checked_sub(&value) {
            Some(b) => b,
            None => return Err("total reserved token too low to unreserve"),
        };
        // set to storage
        TotalFreeToken::<T>::insert(symbol, new_total_free_token);
        TotalReservedToken::<T>::insert(symbol, new_total_reserved_token);
        FreeToken::<T>::insert(&key, new_free_token);
        ReservedToken::<T>::insert(&key, new_reserved_token);

        Self::deposit_event(RawEvent::UnreverseToken(who.clone(), symbol.clone(), value));
        Ok(())
    }

    pub fn move_free_token(from: &T::AccountId, to: &T::AccountId, symbol: &Symbol, value: T::TokenBalance) -> StdResult<(), TokenErr> {
        Self::is_valid_token_for(from, symbol).map_err(|_| TokenErr::InvalidToken)?;
        <T as balances::Trait>::EnsureAccountLiquid::ensure_account_liquid(from).map_err(|_| TokenErr::InvalidAccount)?;
        //TODO validator`

        // for chainx
        if symbol.as_slice() == T::CHAINX_SYMBOL {
            let value: T::Balance = As::sa(value.as_() as u64); // change to balance for balances module
            let from_token: T::Balance = balances::FreeBalance::<T>::get(from);
            let to_token: T::Balance = balances::FreeBalance::<T>::get(to);

            let new_from_token = match from_token.checked_sub(&value) {
                Some(b) => b,
                None => return Err(TokenErr::NotEnough),
            };
            let new_to_token = match to_token.checked_add(&value) {
                Some(b) => b,
                None => return Err(TokenErr::OverFlow),
            };
            balances::FreeBalance::<T>::insert(from, new_from_token);
            balances::FreeBalance::<T>::insert(to, new_to_token);
            Self::deposit_event(RawEvent::MoveFreeToken(from.clone(), to.clone(), symbol.clone(), As::sa(value.as_())));
            return Ok(());
        }

        Self::init_token_for(to, symbol);
        let key_from = (from.clone(), symbol.clone());
        let key_to = (to.clone(), symbol.clone());

        let from_token: T::TokenBalance = FreeToken::<T>::get(&key_from);
        let to_token: T::TokenBalance = FreeToken::<T>::get(&key_to);

        let new_from_token = match from_token.checked_sub(&value) {
            Some(b) => b,
            None => return Err(TokenErr::NotEnough),
        };
        let new_to_token = match to_token.checked_add(&value) {
            Some(b) => b,
            None => return Err(TokenErr::OverFlow),
        };
        FreeToken::<T>::insert(key_from, new_from_token);
        FreeToken::<T>::insert(key_to, new_to_token);
        Self::deposit_event(RawEvent::MoveFreeToken(from.clone(), to.clone(), symbol.clone(), value));
        Ok(())
    }
}

#[derive(PartialEq, Eq, Clone, Encode, Decode)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize, Debug))]
pub enum TokenErr {
    NotEnough,
    OverFlow,
    InvalidToken,
    InvalidAccount,
}

impl TokenErr {
    pub fn info(&self) -> &'static str {
        match *self {
            TokenErr::NotEnough => "free token too low",
            TokenErr::OverFlow => "overflow for this value",
            TokenErr::InvalidToken => "not a valid token for this account",
            TokenErr::InvalidAccount => "Account Locked",
        }
    }
}

impl<T: Trait> Module<T> {
    // public call
    /// transfer token between accountid, notice the fee is chainx
    pub fn transfer_token(origin: T::Origin, dest: balances::Address<T>, sym: Symbol, value: T::TokenBalance) -> Result {
        if sym.as_slice() == T::CHAINX_SYMBOL {
            return Err("not allow to transfer chainx use transfer_token");
        }
        let transactor = ensure_signed(origin)?;
        Self::is_valid_token_for(&transactor, &sym)?;
        let dest = <balances::Module<T>>::lookup(dest)?;
        Self::init_token_for(&dest, &sym);

        let fee = Self::transfer_token_fee();

        let key_from = (transactor.clone(), sym.clone());
        let key_to = (dest.clone(), sym.clone());

        let sender = &transactor;
        let receiver = &dest;
        <cxsupport::Module<T>>::handle_fee_after(sender, fee, true, || {
            // get storage
            let from_token = FreeToken::<T>::get(&key_from);
            let to_token = FreeToken::<T>::get(&key_to);
            // check
            let new_from_token = match from_token.checked_sub(&value) {
                Some(b) => b,
                None => return Err("free token too low to send value"),
            };
            let new_to_token = match to_token.checked_add(&value) {
                Some(b) => b,
                None => return Err("destination free token too high to receive value"),
            };
            if sender != receiver {
                // set to storage
                FreeToken::<T>::insert(&key_from, new_from_token);
                FreeToken::<T>::insert(&key_to, new_to_token);
                Self::deposit_event(RawEvent::TransferToken(sender.clone(), receiver.clone(), sym.clone(), value, fee));
            }
            Ok(())
        })?;

        Ok(())
    }

    pub fn set_transfer_token_fee(val: T::Balance) -> Result {
        <TransferTokenFee<T>>::put(val);
        Self::deposit_event(RawEvent::SetTransferTokenFee(val));
        Ok(())
    }
}
