use crate::btc_chain::Transaction;
use crate::btc_keys::Public;
use crate::btc_primitives::bytes::Bytes;
use crate::btc_script::{
    script::Script, SignatureChecker, SignatureVersion, TransactionInputSigner,
    TransactionSignatureChecker,
};
use crate::rstd::prelude::Vec;
use crate::rstd::result::Result as StdResult;
use crate::support::dispatch::Result;
use crate::types::RelayTx;
use crate::{Module, Trait};

use merkle::parse_partial_merkle_tree;

use crate::xsupport::{debug, error};

pub fn validate_transaction<T: Trait>(tx: &RelayTx) -> Result {
    let tx_hash = tx.raw.hash();

    let header_info = Module::<T>::block_header_for(&tx.block_hash).ok_or_else(|| {
        error!(
            "[validate_transaction]|tx's block header must exist before|block_hash:{:}",
            tx.block_hash
        );
        "tx's block header must exist before"
    })?;

    debug!(
        "[validate_transaction]|relay tx:{:?}|header:{:?}",
        tx, header_info.header
    );

    let merkle_root = header_info.header.merkle_root_hash;
    // verify merkle proof
    match parse_partial_merkle_tree(tx.merkle_proof.clone()) {
        Ok(parsed) => {
            if merkle_root != parsed.root {
                return Err("Check failed for merkle tree proof");
            }
            if !parsed.hashes.iter().any(|h| *h == tx_hash) {
                return Err("Tx hash should in ParsedPartialMerkleTree");
            }
        }
        Err(_) => return Err("Parse partial merkle tree failed"),
    }

    // verify prev tx for input
    // only check the first(0) input in transaction
    let previous_txid = tx.previous_raw.hash();
    if previous_txid != tx.raw.inputs[0].previous_output.hash {
        error!("[validate_transaction]|relay previou tx's hash not equail to relay tx first input|relaytx:{:?}", tx);
        return Err("Previous tx id not equal input point hash");
    }
    Ok(())
}

fn verify_sig(sig: &Bytes, pubkey: &Bytes, tx: &Transaction, script_pubkey: &Bytes) -> bool {
    let tx_signer: TransactionInputSigner = tx.clone().into();
    let checker = TransactionSignatureChecker {
        input_index: 0,
        input_amount: 0,
        signer: tx_signer,
    };
    let sighashtype = 1; // Sighsh all
    let signature = sig.clone().take().into();
    let public = if let Ok(public) = Public::from_slice(pubkey.as_slice()) {
        public
    } else {
        return false;
    };

    //privous tx's output script_pubkey
    let script_code: Script = script_pubkey.clone().into();
    return checker.check_signature(
        &signature,
        &public,
        &script_code,
        sighashtype,
        SignatureVersion::Base,
    );
}

/// Check signed transactions
pub fn parse_and_check_signed_tx<T: Trait>(
    tx: &Transaction,
) -> StdResult<Vec<Bytes>, &'static str> {
    // parse sigs from transaction first input
    let script: Script = tx.inputs[0].script_sig.clone().into();
    if script.len() < 2 {
        return Err("Invalid signature, script_sig is too short");
    }
    let (sigs, _) = script
        .extract_multi_scriptsig()
        .map_err(|_| "Invalid signature")?;
    // parse pubkeys from trustee hot_redeem_script
    let trustee_info =
        Module::<T>::trustee_info().ok_or("Should set trustee address info first.")?;
    let redeem_script = Script::from(trustee_info.hot_redeem_script);
    let (pubkeys, _, _) = redeem_script
        .parse_redeem_script()
        .ok_or("Parse redeem script failed")?;

    let bytes_sedeem_script = redeem_script.to_bytes();
    for sig in sigs.iter() {
        let mut verify = false;
        for pubkey in pubkeys.iter() {
            if verify_sig(sig, pubkey, tx, &bytes_sedeem_script) {
                verify = true;
                break;
            }
        }
        if !verify {
            error!("[parse_and_check_signed_tx]|Verify sign failed|tx:{:?}", tx);
            return Err("Verify sign failed");
        }
    }

    Ok(sigs)
}
