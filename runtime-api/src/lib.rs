// Copyright 2018 Chainpool.
#![cfg_attr(not(feature = "std"), no_std)]

extern crate sr_std as rstd;
extern crate substrate_client as client;

extern crate srml_support;

extern crate chainx_primitives;
extern crate xr_primitives;

extern crate xrml_xassets_assets as xassets;
extern crate xrml_xassets_process as xprocess;
extern crate xrml_xassets_records as xrecords;
extern crate xrml_xdex_spot as xspot;

pub mod xassets_api {
    use chainx_primitives::{AccountId, Balance, Timestamp};
    use client::decl_runtime_apis;
    use rstd::prelude::Vec;
    use xassets::{Memo, Token};
    use xrecords::AddrStr;
    decl_runtime_apis! {
        pub trait XAssetsApi {
            fn valid_assets() -> Vec<Token>;

            fn withdrawal_list_of(chain: xassets::Chain) -> Vec<xrecords::Application<AccountId, Balance, Timestamp>>;

            fn verify_address(token: Token, addr: AddrStr, ext: Memo) -> Result<(), Vec<u8>>;

            fn minimal_withdrawal_value(token: Token) -> Option<Balance>;
        }
    }
}

pub mod xmining_api {
    use chainx_primitives::AccountId;
    use client::decl_runtime_apis;
    use rstd::prelude::Vec;
    use xassets::Token;
    decl_runtime_apis! {
        pub trait XMiningApi {
            fn jackpot_accountid_for(who: AccountId) -> AccountId;
            fn multi_jackpot_accountid_for(who: Vec<AccountId>) -> Vec<AccountId>;
            fn token_jackpot_accountid_for(token: Token) -> AccountId;
            fn multi_token_jackpot_accountid_for(token: Vec<Token>) -> Vec<AccountId>;
        }
    }
}

pub mod xspot_api {
    use chainx_primitives::Balance;
    use client::decl_runtime_apis;
    use xassets::Token;
    decl_runtime_apis! {
        pub trait XSpotApi {
            fn aver_asset_price(token: Token) -> Option<Balance>;
        }
    }
}
