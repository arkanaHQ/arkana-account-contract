use borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::collections::{UnorderedMap};
use near_sdk::json_types::{U128};
use near_sdk::{
    env, ext_contract, near_bindgen, PanicOnDefault, AccountId, Balance, Promise, PromiseResult, PublicKey, Gas,
};

mod models;
use models::*;

#[near_bindgen]
#[derive(PanicOnDefault, BorshDeserialize, BorshSerialize)]
pub struct LinkDrop {
    pub accounts: UnorderedMap<PublicKey, Balance>,
}

/// Gas attached to the callback from account creation.
pub const ON_CREATE_ACCOUNT_CALLBACK_GAS: Gas = Gas(13_000_000_000_000);

#[ext_contract(ext_self)]
pub trait ExtLinkDrop {
    /// Callback after plain account creation.
    fn on_account_created(&mut self, predecessor_account_id: AccountId, amount: U128) -> bool;

    /// Callback after creating account and claiming linkdrop.
    fn on_account_created_and_claimed(&mut self, amount: U128) -> bool;
}

fn is_promise_success() -> bool {
    assert_eq!(
        env::promise_results_count(),
        1,
        "Contract expected a result on the callback"
    );
    match env::promise_result(0) {
        PromiseResult::Successful(_) => true,
        _ => false,
    }
}

#[near_bindgen]
impl LinkDrop {
    /// Initializes the contract with an empty map for the accounts
    #[init]
    pub fn new() -> Self {
        Self { 
            accounts: UnorderedMap::new(b"a") 
        }
    }

    /// Create new account without linkdrop and deposit passed funds (used for creating sub accounts directly).
    #[payable]
    pub fn create_account_advanced(
        &mut self,
        new_account_id: AccountId,
        options: CreateAccountOptions,
    ) -> Promise {
        let is_some_option = options.contract_bytes.is_some() || options.full_access_keys.is_some() || options.limited_access_keys.is_some();
        assert!(is_some_option, "Cannot create account with no options. Please specify either contract bytes, full access keys, or limited access keys.");

        let amount = env::attached_deposit();

        // Initiate a new promise on the new account we're creating and transfer it any attached deposit
        let mut promise = Promise::new(new_account_id).create_account().transfer(amount);
        
        // If there are any full access keys in the options, loop through and add them to the promise
        if let Some(full_access_keys) = options.full_access_keys {
            for key in full_access_keys {
                promise = promise.add_full_access_key(key.clone());
            }
        }

        // If there are any function call access keys in the options, loop through and add them to the promise
        if let Some(limited_access_keys) = options.limited_access_keys {
            for key_info in limited_access_keys {
                promise = promise.add_access_key(key_info.public_key.clone(), key_info.allowance.0, key_info.receiver_id.clone(), key_info.method_names.clone());
            }
        }

        // If there are any contract bytes, we should deploy the contract to the account
        if let Some(bytes) = options.contract_bytes {
            promise = promise.deploy_contract(bytes);
        };

        // Callback if anything went wrong, refund the predecessor for their attached deposit
        promise.then(
            Self::ext(env::current_account_id())
                .with_static_gas(ON_CREATE_ACCOUNT_CALLBACK_GAS)
                .on_account_created(
                    env::predecessor_account_id(),
                    amount.into()
                )
        )
    }

    /// Callback after executing `create_account` or `create_account_advanced`.
    pub fn on_account_created(&mut self, predecessor_account_id: AccountId, amount: U128) -> bool {
        assert_eq!(
            env::predecessor_account_id(),
            env::current_account_id(),
            "Callback can only be called from the contract"
        );
        let creation_succeeded = is_promise_success();
        if !creation_succeeded {
            // In case of failure, send funds back.
            Promise::new(predecessor_account_id).transfer(amount.into());
        }
        creation_succeeded
    }

    /// Callback after execution `create_account_and_claim`.
    pub fn on_account_created_and_claimed(&mut self, amount: U128) -> bool {
        assert_eq!(
            env::predecessor_account_id(),
            env::current_account_id(),
            "Callback can only be called from the contract"
        );
        let creation_succeeded = is_promise_success();
        if creation_succeeded {
            Promise::new(env::current_account_id()).delete_key(env::signer_account_pk());
        } else {
            // In case of failure, put the amount back.
            self.accounts
                .insert(&env::signer_account_pk(), &amount.into());
        }
        creation_succeeded
    }

    /// Returns the balance associated with given key.
    pub fn get_key_balance(&self, key: PublicKey) -> U128 {
        self.accounts.get(&key.into()).expect("Key is missing").into()
    }

    /// Returns information associated with a given key.
    /// Part of the linkdrop NEP
    #[handle_result]
    pub fn get_key_information(&self, key: PublicKey) -> Result<KeyInfo, &'static str> {
        match self.accounts.get(&key) {
            Some(balance) => Ok(KeyInfo { balance: U128(balance) }),
            None => Err("Key is missing"),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {}
