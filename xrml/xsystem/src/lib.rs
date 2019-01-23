// Copyright 2018 Chainpool.

//! this module is for chainx system

#![cfg_attr(not(feature = "std"), no_std)]

extern crate parity_codec as codec;

// for substrate
#[cfg(feature = "std")]
extern crate substrate_primitives;

#[cfg(feature = "std")]
extern crate sr_io as runtime_io;
extern crate sr_primitives as runtime_primitives;
// for substrate runtime module lib
// Needed for type-safe access to storage DB.
#[macro_use]
extern crate srml_support as runtime_support;
extern crate srml_system as system;

#[cfg(test)]
mod tests;

use runtime_support::dispatch::Result;
use runtime_support::StorageValue;

use system::ensure_inherent;

pub trait Trait: system::Trait {
    const XSYSTEM_SET_POSITION: u32;
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        fn set_block_producer(origin, producer: T::AccountId) -> Result {
            ensure_inherent(origin)?;

            assert!(
                <system::Module<T>>::extrinsic_index() == Some(T::XSYSTEM_SET_POSITION),
                "BlockProducer extrinsic must be at position {} in the block",
                T::XSYSTEM_SET_POSITION
            );

            BlockProducer::<T>::put(producer);
            Ok(())
        }
        fn on_finalise(_n: T::BlockNumber) {
            BlockProducer::<T>::kill();
        }
    }
}

decl_storage! {
    trait Store for Module<T: Trait> as XSystem {
        pub BlockProducer get(block_producer): Option<T::AccountId>;
        pub DeathAccount get(death_account) config(): T::AccountId;
        pub BannedAccount get(banned_account) config(): T::AccountId;
        // TODO remove this to other module
        pub BurnAccount get(burn_account) config(): T::AccountId;
    }
}
