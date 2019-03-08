use crate::btc_keys::DisplayLayout;
#[cfg(feature = "std")]
use crate::hash_strip;
use crate::rstd::prelude::Vec;
use crate::runtime_primitives::traits::As;
use crate::tx::handler::parse_deposit_outputs;
use crate::types::{TxType, VoteResult};
use crate::xassets::assetdef::{Chain, ChainT};
use crate::xrecords::{self, RecordInfo, TxState};
use crate::xsupport::error;
use crate::{Module, Trait};

impl<T: Trait> Module<T> {
    pub fn withdrawal_list() -> Vec<RecordInfo<T::AccountId, T::Balance, T::Moment>> {
        let mut records = Vec::new();

        let applications = xrecords::Module::<T>::withdrawal_applications(Chain::Bitcoin);
        for appl in applications {
            let info = RecordInfo {
                who: appl.applicant(),
                token: appl.token(),
                balance: appl.balance(),
                txid: Vec::new(),
                addr: appl.addr(), // for btc, it's bas58 addr
                ext: appl.ext(),
                time: appl.time(),
                withdrawal_id: appl.id(), // only for withdrawal
                state: Default::default(),
            };
            records.push(info)
        }

        match Self::withdrawal_proposal() {
            None => {
                for appl in records.iter_mut() {
                    appl.state = TxState::Applying;
                }
            }
            Some(proposal) => {
                let best = Self::best_index();
                let header_info = if let Some(header_info) = Self::block_header_for(&best) {
                    header_info
                } else {
                    error!(
                        "[withdrawal_list] error!, could not find block for this hash[{:}]!",
                        hash_strip(&best),
                    );
                    return Vec::new();
                };

                // find proposal txhash
                let confirmations = Module::<T>::confirmation_number();
                let mut prev_hash = header_info.header.previous_header_hash.clone();
                let mut tx_hash: Vec<u8> = Default::default();
                for _ in 1..confirmations {
                    if let Some(info) = Module::<T>::block_header_for(prev_hash) {
                        for txid in info.txid_list {
                            if let Some(tx_info) = Self::tx_for(&txid) {
                                if tx_info.tx_type == TxType::Withdraw {
                                    tx_hash = tx_info.raw_tx.hash().as_ref().to_vec();
                                }
                            }
                        }
                        prev_hash = info.header.previous_header_hash
                    } else {
                        error!(
                            "[withdrawal_list] error!, could not find block for this hash[{:}]!",
                            hash_strip(&best),
                        );
                        return Vec::new();
                    }
                }
                for record in records.iter_mut() {
                    record.txid = tx_hash.clone();
                    if proposal
                        .withdrawal_id_list
                        .iter()
                        .any(|id| *id == record.withdrawal_id)
                    {
                        record.state = match proposal.sig_state {
                            VoteResult::Unfinish => TxState::Signing,
                            VoteResult::Finish => TxState::Processing,
                        };
                    }
                    // else the record.state is Default value, it's NotApplying
                }
            }
        }

        records
    }

    pub fn deposit_list() -> Vec<RecordInfo<T::AccountId, T::Balance, T::Moment>> {
        let mut records = Vec::new();

        let best = Self::best_index();
        let header_info = if let Some(header_info) = Self::block_header_for(&best) {
            header_info
        } else {
            error!(
                "[deposit_list] error!, could not find block for this hash[{:}]!",
                hash_strip(&best),
            );
            return Vec::new();
        };

        // find proposal txhash
        let confirmations = Module::<T>::confirmation_number();
        let mut prev_hash = header_info.header.previous_header_hash.clone();
        for i in 1..confirmations {
            if let Some(info) = Module::<T>::block_header_for(prev_hash) {
                for txid in info.txid_list {
                    if let Some(tx_info) = Self::tx_for(&txid) {
                        if tx_info.tx_type == TxType::Deposit {
                            let timestamp = info.header.time;
                            let state = TxState::Confirming(i - 1, confirmations);

                            let r = match parse_deposit_outputs::<T>(&tx_info.raw_tx) {
                                Ok(r) => r,
                                Err(_e) => {
                                    error!("[deposit_list] error!, parse deposit outputs error, info: {:?}", _e);
                                    return Vec::new();
                                }
                            };

                            let (account_info, balance, ext) = r;

                            let tx_hash = tx_info.raw_tx.hash();
                            let info = RecordInfo::<T::AccountId, T::Balance, T::Moment> {
                                who: account_info.map(|(a, _)| a).unwrap_or_default(),
                                token: Self::TOKEN.to_vec(),
                                balance: As::sa(balance),
                                txid: tx_hash.as_ref().to_vec(),
                                addr: Self::input_addr_for(tx_hash)
                                    .unwrap_or_default()
                                    .layout()
                                    .to_vec(),
                                ext: ext,
                                time: As::sa(timestamp as u64),
                                withdrawal_id: 0, // only for withdrawal
                                state: state,
                            };
                            records.push(info);
                        }
                    }
                }
                prev_hash = info.header.previous_header_hash
            } else {
                error!(
                    "[deposit_list] error!, could not find block for this hash[{:}]!",
                    hash_strip(&best),
                );
                return Vec::new();
            }
        }

        records
    }
}
