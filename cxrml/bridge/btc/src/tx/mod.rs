// Copyright 2018 Chainpool

use rstd::prelude::*;
use rstd::marker::PhantomData;
use rstd::result::Result as StdResult;
use codec::Decode;

use runtime_support::dispatch::Result;
use runtime_support::{StorageMap, StorageValue};

use primitives::hash::H256;
use primitives::bytes::Bytes;
use chain::{Transaction, OutPoint, TransactionOutput, TransactionInput};
use merkle::{PartialMerkleTree, parse_partial_merkle_tree};
use runtime_io;
use script::script::Script;
use script::{SignatureChecker, builder, TransactionSignatureChecker, TransactionInputSigner,
             SignatureVersion};
use keys;
use keys::DisplayLayout;
use b58::from;
use timestamp;
use runtime_primitives::traits::As;
use super::{TxType, Trait, ReceiveAddress, NetworkId, RedeemScript, AddressMap, UTXOSet, TxSet,
            BlockTxids, BlockHeaderFor, NumberForHash, UTXOMaxIndex, AccountsMaxIndex,
            AccountsSet, TxProposal, CandidateTx};
pub use self::proposal::{handle_proposal, Proposal};

mod proposal;

#[derive(Clone, Encode, Decode)]
pub struct RelayTx {
    pub block_hash: H256,
    pub raw: Transaction,
    pub merkle_proof: PartialMerkleTree,
    pub previous_raw: Transaction,
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, Default)]
pub struct UTXO {
    pub txid: H256,
    pub index: u32,
    pub balance: u64,
    pub is_spent: bool,
}

//#[derive(PartialEq, Clone, Encode, Decode, Default)]
//pub struct Accounts {
//    pub txid: H256,
//    pub btcaddr: keys::Address,
//    pub chainxaddr: AccountId,
//    pub regtimestamp: u32,
//    pub regtype: TxType,
//}

struct UTXOStorage<T: Trait>(PhantomData<T>);

impl<T: Trait> UTXOStorage<T> {
    // To do: Optmise select OutPoint algorithm
    fn select_utxo(balance: u64) -> Option<Vec<UTXO>> {
        let mut index = <UTXOMaxIndex<T>>::get();
        let mut count_balance = 0;
        let mut utxo_set = Vec::new();
        while index > 0 {
            index -= 1;
            let utxo = <UTXOSet<T>>::get(index);
            if utxo.is_spent == false {
                utxo_set.push(utxo.clone());
                count_balance += utxo.balance;
                if count_balance >= balance {
                    return Some(utxo_set);
                }
            }
        }
        None
    }

    fn add(new_utxos: Vec<UTXO>) {
        let mut index = <UTXOMaxIndex<T>>::get();
        for utxo in new_utxos {
            <UTXOSet<T>>::insert(index, utxo.clone());
            index += 1;
        }
        <UTXOMaxIndex<T>>::mutate(|inc| *inc = index);
    }

    fn update(utxos: Vec<UTXO>, is_spent: bool) -> u64 {
        let mut update_balance = 0;
        for u in utxos {
            let mut index = <UTXOMaxIndex<T>>::get();
            while index > 0 {
                index -= 1;
                let utxo = <UTXOSet<T>>::get(index);
                if utxo == u {
                    <UTXOSet<T>>::mutate(index, |utxo| {
                        update_balance += utxo.balance;
                        utxo.is_spent = is_spent;
                    });
                    break;
                }
            }
        }
        update_balance
    }

    fn update_from_outpoint(out_point_set: Vec<OutPoint>, is_spent: bool) -> u64 {
        let mut update_balance = 0;
        for out_point in out_point_set {
            let mut index = <UTXOMaxIndex<T>>::get();
            while index > 0 {
                index -= 1;
                let utxo = <UTXOSet<T>>::get(index);
                if out_point.hash == utxo.txid && out_point.index == utxo.index {
                    <UTXOSet<T>>::mutate(index, |utxo| {
                        update_balance += utxo.balance;
                        utxo.is_spent = is_spent;
                    });
                    break;
                }
            }
        }
        update_balance
    }

    fn find_utxo(out_point: &OutPoint) -> Option<UTXO> {
        let mut index = <UTXOMaxIndex<T>>::get();
        while index > 0 {
            index -= 1;
            let utxo = <UTXOSet<T>>::get(index);
            if out_point.hash == utxo.txid && out_point.index == utxo.index {
                return Some(utxo);
            }
        }
        None
    }
}


pub trait RollBack<T: Trait> {
    fn rollback_tx(header: &H256) -> Result;
}

pub struct TxStorage<T: Trait>(PhantomData<T>);

impl<T: Trait> TxStorage<T> {
    fn store_tx(
        tx: &Transaction,
        block_hash: &H256,
        who: &T::AccountId,
        address: keys::Address,
        tx_type: TxType,
        balance: u64,
    ) {
        let hash = tx.hash();
        let block_hash = block_hash.clone();
        let tx = tx.clone();

        // todo 检查block是否存在
        <BlockTxids<T>>::mutate(block_hash, |v| v.push(hash.clone()));

        <TxSet<T>>::insert(hash, (who.clone(), address, tx_type, balance, tx));
    }

    fn check_previous(txid: &H256) -> bool {
        <TxSet<T>>::exists(txid)
    }

    fn find_tx(txid: &H256) -> Option<Transaction> {
        if let Some(tuple) = <TxSet<T>>::get(txid) {
            return Some(tuple.4);
        } else {
            return None;
        }
    }
}

impl<T: Trait> RollBack<T> for TxStorage<T> {
    fn rollback_tx(header: &H256) -> Result {
        let receive_address = if let Some(h) = <ReceiveAddress<T>>::get() {
            h
        } else {
            return Err("should set RECEIVE_ADDRESS first");
        };

        let txids = <BlockTxids<T>>::get(header);
        for txid in txids.iter() {
            let (_, _, tx_type, _, tx) = <TxSet<T>>::get(txid).unwrap();
            match tx_type {
                TxType::Withdraw => {
                    let out_point_set = tx.inputs
                        .iter()
                        .map(|input| input.previous_output.clone())
                        .collect();
                    <UTXOStorage<T>>::update_from_outpoint(out_point_set, false);
                    let receive_address =
                        if let Ok(receive_address) = keys::Address::from_layout(&receive_address) {
                            receive_address
                        } else {
                            return Err("Invalid Address");
                        };
                    let mut out_point_set2: Vec<OutPoint> = Vec::new();
                    let mut index = 0;
                    let _ = tx.outputs.iter().map(|output| {
                        if is_key(&output.script_pubkey, &receive_address) {
                            out_point_set2.push(OutPoint {
                                hash: txid.clone(),
                                index: index,
                            })
                        }
                        index += 1;
                        ()
                    }).collect::<()>();
                    <UTXOStorage<T>>::update_from_outpoint(out_point_set2, true);
                }
                TxType::Register => {}
                _ => {
                    let mut rollback_spent: Vec<UTXO> = Vec::new();
                    for (index, output) in tx.outputs.iter().enumerate() {
                        rollback_spent.push(UTXO {
                            txid: tx.hash(),
                            index: index as u32,
                            balance: output.value,
                            is_spent: false,
                        });
                    }
                    <UTXOStorage<T>>::update(rollback_spent, true);
                }
            }
        }
        Ok(())
    }
}

struct AccountsStorage<T: Trait>(PhantomData<T>);

impl<T: Trait> AccountsStorage<T> {
    fn add(accounts: (H256, keys::Address, T::AccountId, u32, TxType)) {
        let mut index = <AccountsMaxIndex<T>>::get();
        <AccountsSet<T>>::insert(index, accounts.clone());
        index += 1;
        <AccountsMaxIndex<T>>::mutate(|inc| *inc = index);
    }
}

pub fn validate_transaction<T: Trait>(
    tx: &RelayTx,
    receive_address: &[u8],
) -> StdResult<TxType, &'static str> {
    if <NumberForHash<T>>::exists(&tx.block_hash) == false {
        return Err("this tx's block not in the main chain");
    }

    let select_header = if let Some((header, _, _)) = <BlockHeaderFor<T>>::get(&tx.block_hash) {
        header
    } else {
        return Err("not has this block header yet");
    };

    let merkle_root = select_header.merkle_root_hash;
    let verify_txid = tx.raw.hash();
    // Verify proof
    match parse_partial_merkle_tree(tx.merkle_proof.clone()) {
        Ok(parsed) => {
            if merkle_root != parsed.root {
                return Err("check failed for merkle tree proof");
            }
            if !parsed.hashes.iter().any(|h| *h == verify_txid) {
                return Err("txid should in ParsedPartialMerkleTree");
            }
        }
        Err(_) => return Err("parse partial merkle tree failed"),
    }
    let previous_txid = tx.previous_raw.hash();
    if previous_txid != tx.raw.inputs[0].previous_output.hash {
        return Err("previous tx id not right");
    }
    let receive_address =
        if let Ok(receive_address) = keys::Address::from_layout(&receive_address) {
            receive_address
        } else {
            return Err("Invalid Address");
        };

    // detect withdraw
    for input in tx.raw.inputs.iter() {
        let outpoint = input.previous_output.clone();
        let send_address = inspect_address::<T>(&tx.previous_raw, outpoint).unwrap();
        if send_address.hash == receive_address.hash {
            return Ok(TxType::Withdraw);
        }
    }
    // detect deposit
    for output in tx.raw.outputs.iter() {
        if is_key(&output.script_pubkey, &receive_address) {
            return Ok(TxType::RegisterDeposit);
        }
    }

    Err("not found our pubkey, may be an unrelated tx")
}

fn is_key(script_pubkey: &[u8], receive_address: &keys::Address) -> bool {
    runtime_io::print("------script");
    runtime_io::print(script_pubkey);
    let script: Vec<u8> = script_pubkey.iter().cloned().collect();
    let script: Script = script.into();
    let script_addresses = script.extract_destinations().unwrap_or(Vec::new());
    if script_addresses.len() == 1 {
        if receive_address.hash == script_addresses[0].hash {
            return true;
        }
    }
    return false;
}

pub fn handle_input<T: Trait>(
    tx: &Transaction,
    block_hash: &H256,
    who: &T::AccountId,
    receive_address: &[u8],
) {
    let out_point_set = tx.inputs
        .iter()
        .map(|input| input.previous_output.clone())
        .collect();
    let mut update_balance = <UTXOStorage<T>>::update_from_outpoint(out_point_set, true);
    let receive_address =
        if let Ok(receive_address) = keys::Address::from_layout(&receive_address) {
            receive_address
        } else {
            return ();
        };

    let _ = tx.outputs.iter().map(|output| {
        if is_key(&output.script_pubkey, &receive_address) {
            update_balance -= output.value;
        }
        ()
    }).collect::<()>();
    <TxStorage<T>>::store_tx(
        tx,
        block_hash,
        who,
        receive_address.clone(),
        TxType::Withdraw,
        update_balance,
    );
}

fn deposit_token<T: Trait>(_address: keys::Address, _balance: u64) {
    // to do:
}

fn inspect_address<T: Trait>(tx: &Transaction, outpoint: OutPoint) -> Option<keys::Address> {
    let script: Script = tx.outputs[outpoint.index as usize]
        .script_pubkey
        .clone()
        .into();
    let script_addresses = script.extract_destinations().unwrap_or(Vec::new());
    if script_addresses.len() == 1 {
        let address = &script_addresses[0];
        let network_id = <NetworkId<T>>::get();
        let network = if network_id == 1 {
            keys::Network::Testnet
        } else {
            keys::Network::Mainnet
        };
        let address = keys::Address {
            kind: address.kind,
            network: network,
            hash: address.hash.clone(),
        };
        return Some(address);
    }
    return None;
}

pub fn handle_output<T: Trait>(
    tx: &Transaction,
    block_hash: &H256,
    who: &T::AccountId,
    previous_tx: &Transaction,
    receive_address: &[u8],
) -> Vec<UTXO> {
    let mut new_utxos = Vec::<UTXO>::new();
    let mut total_balance: u64 = 0;
    let mut register = false;
    let mut is_in_accounts = false;
    // Add utxo
    let receive_address =
        if let Ok(receive_address) = keys::Address::from_layout(&receive_address) {
            receive_address
        } else {
            return Vec::new();
        };
    let outpoint = tx.inputs[0].previous_output.clone();
    let send_address = inspect_address::<T>(previous_tx, outpoint).unwrap();
    for (index, output) in tx.outputs.iter().enumerate() {
        let script = &output.script_pubkey;
        let script: Script = script.clone().into();
        // bind address [btc address --> chainx AccountId]
        if script.is_null_data_script() {
            let data = script.extract_rear(':');
            runtime_io::print("------opreturn account data-----");
            runtime_io::print(&data[..]);
            let account = from(data).unwrap();
            let id: T::AccountId = Decode::decode(&mut account.as_slice()).unwrap();
            match <AddressMap<T>>::get(send_address.clone()) {
                Some(_a) => is_in_accounts = true,
                None => {
                    runtime_io::print("------new account-----");
                    is_in_accounts = false
                }
            };
            <AddressMap<T>>::insert(send_address.clone(), id);
            register = true;
            continue;
        }
        // deposit money
        let script_addresses = script.extract_destinations().unwrap_or(Vec::new());
        if script_addresses.len() == 1 {
            if receive_address.hash == script_addresses[0].hash {
                runtime_io::print("------deposit_token-----");
                deposit_token::<T>(send_address.clone(), output.value);
                total_balance += output.value;
                new_utxos.push(UTXO {
                    txid: tx.hash(),
                    index: index as u32,
                    balance: output.value,
                    is_spent: false,
                });
            }
        }
    }
    if total_balance == 0 {
        <TxStorage<T>>::store_tx(
            tx,
            block_hash,
            who,
            send_address.clone(),
            TxType::Register,
            total_balance,
        );
    } else {
        let tx_type = if register {
            <TxStorage<T>>::store_tx(
                tx,
                block_hash,
                who,
                send_address.clone(),
                TxType::RegisterDeposit,
                total_balance,
            );
            TxType::RegisterDeposit
        } else {
            <TxStorage<T>>::store_tx(
                tx,
                block_hash,
                who,
                send_address.clone(),
                TxType::Deposit,
                total_balance,
            );
            TxType::Deposit
        };
        <UTXOStorage<T>>::add(new_utxos.clone());

        if !is_in_accounts {
            let now: T::Moment = <timestamp::Now<T>>::get();
            let current_time: u32 = now.as_() as u32;
            let chainxaddr = <AddressMap<T>>::get(send_address.clone()).unwrap();
            let account = (
                tx.hash(),
                send_address.clone(),
                chainxaddr,
                current_time,
                tx_type,
            );
            <AccountsStorage<T>>::add(account);
            runtime_io::print("------insert new account in AccountsMap-----");
        }
    }
    runtime_io::print("------store tx success-----");
    new_utxos
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accountId() {
        let script = Script::from(
            "chainx:mjKE11gjVN4JaC9U8qL6ZB5vuEBgmwik7b"
                .as_bytes()
                .to_vec(),
        );
        let data = script.extract_rear(':');
        println!("data :{:?}", data);
        let account = from(data).unwrap();
        println!("account :{:?}", account);
        let id: u64 = Decode::decode(&mut account.as_slice()).unwrap();
        println!("id :{:?}", id);
    }
}
