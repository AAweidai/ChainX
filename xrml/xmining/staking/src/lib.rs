// Copyright 2018 Chainpool.
//! Staking manager: Periodically determines the best set of validators.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
use serde_derive::{Deserialize, Serialize};

use parity_codec as codec;

use codec::Compact;
use parity_codec_derive::{Decode, Encode};
use primitives::traits::{As, Lookup, StaticLookup, Zero};
use rstd::prelude::*;
use runtime_support::{
    decl_event, decl_module, decl_storage, dispatch::Result, ensure, StorageMap, StorageValue,
};
use system::ensure_signed;

use xaccounts::{IntentionJackpotAccountIdFor, Name, TrusteeEntity, TrusteeIntentionProps, URL};
use xassets::{Chain, Memo, Token};
use xr_primitives::XString;
use xsupport::info;

pub mod vote_weight;

mod shifter;

mod mock;

mod tests;

pub use shifter::{OnReward, OnRewardCalculation, RewardHolder};
pub use vote_weight::VoteWeight;

const DEFAULT_MINIMUM_VALIDATOR_COUNT: u32 = 4;

pub enum ClaimType {
    Intention,
    PseduIntention(Token),
}

/// Intention mutable properties
#[derive(PartialEq, Eq, Clone, Encode, Decode, Default)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize, Debug))]
#[cfg_attr(feature = "std", serde(rename_all = "camelCase"))]
pub struct IntentionProfs<Balance: Default, BlockNumber: Default> {
    pub total_nomination: Balance,
    pub last_total_vote_weight: u64,
    pub last_total_vote_weight_update: BlockNumber,
}

/// Nomination record of one of the nominator's nominations.
#[derive(PartialEq, Eq, Clone, Encode, Decode, Default)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize, Debug))]
#[cfg_attr(feature = "std", serde(rename_all = "camelCase"))]
pub struct NominationRecord<Balance, BlockNumber> {
    pub nomination: Balance,
    pub last_vote_weight: u64,
    pub last_vote_weight_update: BlockNumber,
    pub revocations: Vec<(BlockNumber, Balance)>,
}

pub trait Trait:
    xassets::Trait + xaccounts::Trait + xsystem::Trait + session::Trait + xbitcoin::Trait
{
    /// The overarching event type.
    type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;

    /// Need to calculate the reward for non-intentions.
    type OnRewardCalculation: OnRewardCalculation<Self::AccountId, Self::Balance>;

    /// Time to distribute reward
    type OnReward: OnReward<Self::AccountId, Self::Balance>;
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        fn deposit_event<T>() = default;

        /// Transactor could be an intention.
        fn nominate(
            origin,
            target: <T::Lookup as StaticLookup>::Source,
            value: T::Balance,
            memo: Memo
        ) {
            let who = ensure_signed(origin)?;
            let target = system::ChainContext::<T>::default().lookup(target)?;

            xassets::is_valid_memo::<T>(&memo)?;
            ensure!(!value.is_zero(), "Cannot nominate zero.");
            ensure!(
                Self::is_intention(&target),
                "Cannot nominate a non-intention."
            );
            ensure!(
                value <= <xassets::Module<T>>::pcx_free_balance(&who),
                "Cannot nominate if greater than your avaliable free balance."
            );

            Self::apply_nominate(&who, &target, value)?;
        }

        fn unnominate(
            origin,
            target: <T::Lookup as StaticLookup>::Source,
            value: T::Balance,
            memo: Memo
        ) {
            let who = ensure_signed(origin)?;
            let target = system::ChainContext::<T>::default().lookup(target)?;

            xassets::is_valid_memo::<T>(&memo)?;
            ensure!(!value.is_zero(), "Cannot unnominate zero.");
            ensure!(
                <NominationRecords<T>>::get((who.clone(), target.clone())).is_some(),
                "Cannot unnominate if target is not your nominee."
            );
            ensure!(
                value <= Self::revokable_of(&who, &target),
                "Cannot unnominate if greater than your revokable nomination."
            );

            Self::apply_unnominate(&who, &target, value)?;
        }

        fn claim(origin, target: <T::Lookup as StaticLookup>::Source) {
            let who = ensure_signed(origin)?;
            let target = system::ChainContext::<T>::default().lookup(target)?;

            ensure!(
                <NominationRecords<T>>::get((who.clone(), target.clone())).is_some(),
                "Cannot claim if target is not your nominee."
            );

            Self::apply_claim(&who, &target)?;
        }

        fn unfreeze(
            origin,
            target: <T::Lookup as StaticLookup>::Source,
            revocation_index: u32
        ) {
            let who = ensure_signed(origin)?;
            let target = system::ChainContext::<T>::default().lookup(target)?;

            let nominate_pair = (who.clone(), target.clone());

            ensure!(
                <NominationRecords<T>>::get(&nominate_pair).is_some(),
                "Cannot unfreeze if target is not your nominee."
            );

            let record = Self::nomination_record_of(&who, &target);
            let mut revocations = record.revocations;

            ensure!(revocations.len() > 0, "Revocation list is empty");
            ensure!(
                revocation_index < revocations.len() as u32,
                "Revocation index out of range."
            );

            let (block, value) = revocations[revocation_index as usize];
            let current_block = <system::Module<T>>::block_number();
            if current_block < block {
                return Err("The requested revocation is not due yet.");
            }

            Self::staking_unreserve(&who, value)?;

            revocations.swap_remove(revocation_index as usize);
            if let Some(mut record) = <NominationRecords<T>>::get(&nominate_pair) {
                record.revocations = revocations;
                <NominationRecords<T>>::insert(&nominate_pair, record);
            }
            Self::deposit_event(RawEvent::Unfreeze(who, target));
        }

        /// Update the url, desire to join in elections of intention and session key.
        fn refresh(
            origin,
            url: Option<URL>,
            desire_to_run: Option<bool>,
            next_key: Option<T::SessionKey>,
            about: Option<XString>
        ) {
            let who = ensure_signed(origin)?;

            ensure!(Self::is_intention(&who), "Cannot refresh if transactor is not an intention.");

            if let Some(url) = url.as_ref() {
                xaccounts::is_valid_url::<T>(url)?;
            }

            if let Some(about) = about.as_ref() {
                xaccounts::is_valid_about::<T>(about)?;
            }

            if let Some(desire_to_run) = desire_to_run.as_ref() {
                if !desire_to_run {
                    let active = Self::intentions().into_iter()
                        .filter(|n| <xaccounts::Module<T>>::intention_props_of(n).is_active)
                        .collect::<Vec<_>>();
                    if active.len() <= Self::minimum_validator_count() as usize {
                        return Err("Cannot pull out when there are too few active intentions.");
                    }
                }
            }

            Self::apply_refresh(&who, url, desire_to_run, next_key, about);
        }

        /// Register intention
        fn register(origin, name: Name) {
            let who = ensure_signed(origin)?;

            xaccounts::is_valid_name::<T>(&name)?;

            ensure!(!Self::is_intention(&who), "Cannot register if transactor is an intention already.");

            Self::apply_register(&who, name)?;
        }

        fn setup_trustee(origin, chain: Chain, about: XString, hot_entity: TrusteeEntity, cold_entity: TrusteeEntity) {
            let who = ensure_signed(origin)?;

            ensure!(Self::is_intention(&who), "Transactor is not an intention.");
            ensure!(<xaccounts::Module<T>>::intention_props_of(&who).is_active, "Intention must be active.");

            xaccounts::is_valid_about::<T>(&about)?;

            // TODO validate addr
            Self::validate_trustee_entity(&chain, &hot_entity)?;
            Self::validate_trustee_entity(&chain, &cold_entity)?;

            <xaccounts::TrusteeIntentionPropertiesOf<T>>::insert(
                &(who, chain),
                TrusteeIntentionProps {
                    about,
                    hot_entity,
                    cold_entity
                }
            );
        }

        /// Set the number of sessions in an era.
        fn set_sessions_per_era(#[compact] new: T::BlockNumber) {
            <NextSessionsPerEra<T>>::put(new);
        }

        /// The length of the bonding duration in eras.
        fn set_bonding_duration(#[compact] new: T::BlockNumber) {
            <BondingDuration<T>>::put(new);
        }

        /// The ideal number of validators.
        fn set_validator_count(new: Compact<u32>) {
            let new: u32 = new.into();
            <ValidatorCount<T>>::put(new);
        }

        /// Force there to be a new era. This also forces a new session immediately after.
        /// `apply_rewards` should be true for validators to get the session reward.
        fn force_new_era(apply_rewards: bool) -> Result {
            Self::apply_force_new_era(apply_rewards)
        }

        /// Set the offline slash grace period.
        fn set_minimum_penalty(new: T::Balance) {
            <MinimumPenalty<T>>::put(new);
        }

    }
}

/// An event in this module.
decl_event!(
    pub enum Event<T>
    where
        <T as balances::Trait>::Balance,
        <T as consensus::Trait>::SessionKey,
        <T as system::Trait>::AccountId,
        <T as system::Trait>::BlockNumber
    {
        /// All validators have been rewarded by the given balance.
        Reward(Balance, Balance),
        /// One validator (and their nominators) has been slashed by the given amount.
        OfflineSlash(AccountId, Balance),
        OfflineValidator(AccountId),
        EnforceValidatorsInactive(Vec<AccountId>),
        Rotation(Vec<(AccountId, u64)>),
        NewTrustees(Vec<AccountId>),
        Unnominate(BlockNumber),
        Nominate(AccountId, AccountId, Balance),
        Claim(u64, u64, Balance),
        Refresh(Option<URL>, Option<bool>, Option<SessionKey>, Option<XString>),
        Unfreeze(AccountId, AccountId),
    }
);

decl_storage! {
    trait Store for Module<T: Trait> as XStaking {
        pub InitialReward get(initial_reward) config(): T::Balance;

        pub TrusteeCount get(trustee_count) config(): u32;
        pub MinimumTrusteeCount get(minimum_trustee_count) config(): u32;

        /// The ideal number of staking participants.
        pub ValidatorCount get(validator_count) config(): u32;
        /// Minimum number of staking participants before emergency conditions are imposed.
        pub MinimumValidatorCount get(minimum_validator_count) config(): u32 = DEFAULT_MINIMUM_VALIDATOR_COUNT;
        /// The length of a staking era in sessions.
        pub SessionsPerEra get(sessions_per_era) config(): T::BlockNumber = T::BlockNumber::sa(1000);
        /// The length of the bonding duration in blocks.
        pub BondingDuration get(bonding_duration) config(): T::BlockNumber = T::BlockNumber::sa(1000);
        /// The length of the bonding duration in blocks for intention.
        pub IntentionBondingDuration get(intention_bonding_duration) config(): T::BlockNumber = T::BlockNumber::sa(10_000);

        pub SessionsPerEpoch get(sessions_per_epoch) config(): T::BlockNumber = T::BlockNumber::sa(10_000);

        pub ValidatorStakeThreshold get(validator_stake_threshold) config(): T::Balance = T::Balance::sa(1);

        /// The current era index.
        pub CurrentEra get(current_era) config(): T::BlockNumber;
        /// All the accounts with a desire to stake.
        pub Intentions get(intentions): Vec<T::AccountId>;

        /// The next value of sessions per era.
        pub NextSessionsPerEra get(next_sessions_per_era): Option<T::BlockNumber>;
        /// The session index at which the era length last changed.
        pub LastEraLengthChange get(last_era_length_change): T::BlockNumber;

        /// We are forcing a new era.
        pub ForcingNewEra get(forcing_new_era): Option<()>;

        pub StakeWeight get(stake_weight): map T::AccountId => T::Balance;

        pub IntentionProfiles get(intention_profiles): map T::AccountId => IntentionProfs<T::Balance, T::BlockNumber>;

        pub NominationRecords get(nomination_records): map (T::AccountId, T::AccountId) => Option<NominationRecord<T::Balance, T::BlockNumber>>;

        pub TeamAddress get(team_address): T::AccountId;
        pub CouncilAddress get(council_address): T::AccountId;
        /// Minimum penalty for each slash.
        pub MinimumPenalty get(minimum_penalty) config(): T::Balance;
        /// The validators should be slashed per session.
        pub SlashedPerSession get(slashed): Vec<T::AccountId>;
        /// The accumulative fine that each slashed validator should pay per session.
        pub TotalSlashOfPerSession get(total_slash_of_per_session): map T::AccountId => T::Balance;
    }
}

impl<T: Trait> Module<T> {
    // Public immutables
    pub fn revokable_of(source: &T::AccountId, target: &T::AccountId) -> T::Balance {
        Self::nomination_record_of(source, target).nomination
    }

    /// How many votes nominator have nomianted for the nominee.
    pub fn nomination_record_of(
        nominator: &T::AccountId,
        nominee: &T::AccountId,
    ) -> NominationRecord<T::Balance, T::BlockNumber> {
        let mut record = NominationRecord::default();
        record.last_vote_weight_update = <system::Module<T>>::block_number();
        <NominationRecords<T>>::get(&(nominator.clone(), nominee.clone())).unwrap_or(record)
    }

    pub fn total_nomination_of(intention: &T::AccountId) -> T::Balance {
        <IntentionProfiles<T>>::get(intention).total_nomination
    }

    pub fn is_intention(who: &T::AccountId) -> bool {
        <xaccounts::Module<T>>::intention_name_of(who).is_some()
    }

    pub fn validate_trustee_entity(chain: &Chain, entity: &TrusteeEntity) -> Result {
        match chain {
            Chain::Bitcoin => match entity {
                TrusteeEntity::Bitcoin(pubkey) if pubkey.len() != 33 && pubkey.len() != 65 => {
                    return Err("Valid pubkeys are either 33 or 65 bytes.");
                }
                _ => (),
            },
            _ => return Err("Unsupported chain."),
        }

        Ok(())
    }

    // Private mutables

    fn mutate_nomination_record(
        nominator: &T::AccountId,
        nominee: &T::AccountId,
        record: NominationRecord<T::Balance, T::BlockNumber>,
    ) {
        <NominationRecords<T>>::insert(&(nominator.clone(), nominee.clone()), record);
    }

    fn staking_reserve(who: &T::AccountId, value: T::Balance) -> Result {
        <xassets::Module<T>>::pcx_move_balance(
            who,
            xassets::AssetType::Free,
            who,
            xassets::AssetType::ReservedStaking,
            value,
        )
        .map_err(|e| e.info())
    }

    fn unnominate_reserve(who: &T::AccountId, value: T::Balance) -> Result {
        <xassets::Module<T>>::pcx_move_balance(
            who,
            xassets::AssetType::ReservedStaking,
            who,
            xassets::AssetType::ReservedStakingRevocation,
            value,
        )
        .map_err(|e| e.info())
    }

    fn staking_unreserve(who: &T::AccountId, value: T::Balance) -> Result {
        <xassets::Module<T>>::pcx_move_balance(
            who,
            xassets::AssetType::ReservedStakingRevocation,
            who,
            xassets::AssetType::Free,
            value,
        )
        .map_err(|e| e.info())
    }

    // Just force_new_era without origin check.
    fn apply_force_new_era(apply_rewards: bool) -> Result {
        <ForcingNewEra<T>>::put(());
        <session::Module<T>>::apply_force_new_session(apply_rewards)
    }

    fn apply_nominate(source: &T::AccountId, target: &T::AccountId, value: T::Balance) -> Result {
        Self::staking_reserve(source, value)?;
        Self::apply_update_vote_weight(source, target, value, true);
        Self::deposit_event(RawEvent::Nominate(
            source.clone(),
            target.clone(),
            value.clone(),
        ));

        Ok(())
    }

    fn apply_unnominate(source: &T::AccountId, target: &T::AccountId, value: T::Balance) -> Result {
        let freeze_until = if Self::is_intention(source) && *source == *target {
            <system::Module<T>>::block_number() + Self::intention_bonding_duration()
        } else {
            <system::Module<T>>::block_number() + Self::bonding_duration()
        };

        let mut revocations = Self::nomination_record_of(source, target).revocations;

        if let Some(index) = revocations.iter().position(|&n| n.0 == freeze_until) {
            let (freeze_until, old_value) = revocations[index];
            revocations[index] = (freeze_until, old_value + value);
        } else {
            revocations.push((freeze_until, value));
        }

        Self::unnominate_reserve(source, value)?;

        if let Some(mut record) = <NominationRecords<T>>::get(&(source.clone(), target.clone())) {
            record.revocations = revocations;
            <NominationRecords<T>>::insert(&(source.clone(), target.clone()), record);
        }

        Self::apply_update_vote_weight(source, target, value, false);

        Self::deposit_event(RawEvent::Unnominate(freeze_until));

        Ok(())
    }

    fn apply_claim(who: &T::AccountId, target: &T::AccountId) -> Result {
        let mut iprof = <IntentionProfiles<T>>::get(target);
        let mut record = Self::nomination_record_of(who, target);

        let jackpot_addr = T::DetermineIntentionJackpotAccountId::accountid_for(target);
        let (source_vote_weight, target_vote_weight, dividend) = Self::generic_claim(
            &mut record,
            who,
            &mut iprof,
            &jackpot_addr,
            ClaimType::Intention,
        )?;
        Self::deposit_event(RawEvent::Claim(
            source_vote_weight,
            target_vote_weight,
            dividend,
        ));

        <IntentionProfiles<T>>::insert(target, iprof);
        Self::mutate_nomination_record(who, target, record);

        Ok(())
    }

    #[cfg(feature = "std")]
    pub fn bootstrap_refresh(
        who: &T::AccountId,
        url: Option<URL>,
        desire_to_run: Option<bool>,
        next_key: Option<T::SessionKey>,
        about: Option<XString>,
    ) {
        Self::apply_refresh(who, url, desire_to_run, next_key, about)
    }

    fn apply_refresh(
        who: &T::AccountId,
        url: Option<URL>,
        desire_to_run: Option<bool>,
        next_key: Option<T::SessionKey>,
        about: Option<XString>,
    ) {
        if let Some(url) = url.clone() {
            <xaccounts::IntentionPropertiesOf<T>>::mutate(who, |props| {
                props.url = url;
            });
        }

        if let Some(desire_to_run) = desire_to_run {
            <xaccounts::IntentionPropertiesOf<T>>::mutate(who, |props| {
                props.is_active = desire_to_run;
            });
        }

        if let Some(next_key) = next_key.clone() {
            <session::NextKeyFor<T>>::insert(who, next_key);
        }

        if let Some(about) = about.clone() {
            <xaccounts::IntentionPropertiesOf<T>>::mutate(who, |props| {
                props.about = about;
            });
        }

        Self::deposit_event(RawEvent::Refresh(url, desire_to_run, next_key, about));
    }

    #[cfg(feature = "std")]
    pub fn bootstrap_register(intention: &T::AccountId, name: Name) -> Result {
        Self::apply_register(intention, name)
    }

    /// Actually register an intention.
    fn apply_register(intention: &T::AccountId, name: Name) -> Result {
        <xaccounts::IntentionOf<T>>::insert(&name, intention.clone());
        <xaccounts::IntentionNameOf<T>>::insert(intention, name);
        <xaccounts::IntentionPropertiesOf<T>>::insert(
            intention,
            xaccounts::IntentionProps::default(),
        );

        <Intentions<T>>::mutate(|i| i.push(intention.clone()));
        <IntentionProfiles<T>>::insert(
            intention,
            IntentionProfs {
                total_nomination: Zero::zero(),
                last_total_vote_weight: 0,
                last_total_vote_weight_update: <system::Module<T>>::block_number(),
            },
        );

        Ok(())
    }

    #[cfg(feature = "std")]
    pub fn bootstrap_update_vote_weight(
        source: &T::AccountId,
        target: &T::AccountId,
        value: T::Balance,
        to_add: bool,
    ) {
        Self::apply_update_vote_weight(source, target, value, to_add)
    }

    /// Actually update the vote weight and nomination balance of source and target.
    fn apply_update_vote_weight(
        source: &T::AccountId,
        target: &T::AccountId,
        value: T::Balance,
        to_add: bool,
    ) {
        let mut iprof = <IntentionProfiles<T>>::get(target);
        let mut record = Self::nomination_record_of(source, target);

        Self::update_vote_weight_both_way(&mut iprof, &mut record, value.as_(), to_add);

        <IntentionProfiles<T>>::insert(target, iprof);
        Self::mutate_nomination_record(source, target, record);
    }
}

impl<T: Trait> Module<T> {
    pub fn validators() -> Vec<(T::AccountId, u64)> {
        session::Module::<T>::validators()
    }

    pub fn jackpot_accountid_for(who: &T::AccountId) -> T::AccountId {
        T::DetermineIntentionJackpotAccountId::accountid_for(who)
    }

    pub fn multi_jackpot_accountid_for(whos: &Vec<T::AccountId>) -> Vec<T::AccountId> {
        whos.into_iter()
            .map(|who| T::DetermineIntentionJackpotAccountId::accountid_for(who))
            .collect()
    }
}
