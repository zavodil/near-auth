/*
 * This is an example of a Rust smart contract with two simple, symmetric functions:
 *
 * 1. set_greeting: accepts a greeting, such as "howdy", and records it for the user (account_id)
 *    who sent the request
 * 2. get_greeting: accepts an account_id and returns the greeting saved for it, defaulting to
 *    "Hello"
 *
 * Learn more about writing NEAR smart contracts with Rust:
 * https://github.com/near/near-sdk-rs
 *
 */

// To conserve gas, efficient serialization is achieved through Borsh (http://borsh.io/)
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::wee_alloc;
use near_sdk::{env, near_bindgen, AccountId, Promise};
use near_sdk::json_types::Base58PublicKey;
use near_sdk::collections::UnorderedMap;

const ACCESS_KEY_ALLOWANCE: u128 = 10_000_000_000_000_000_000_000; // 0.01

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

// Structs in Rust are similar to other languages, and may include impl keyword as shown below
// Note: the names of the structs are not important when calling the smart contract, but the function names are
#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize)]
pub struct NearAuth {
    accounts: UnorderedMap<AccountId, Vec<Contact>>,
    requests: UnorderedMap<AccountId, Contact>,
}

impl Default for NearAuth {
    fn default() -> Self {
        Self {
            accounts: UnorderedMap::new(b"e".to_vec()),
            requests: UnorderedMap::new(b"p".to_vec()),
        }
    }
}

#[derive(BorshSerialize, BorshDeserialize, Eq, PartialEq, Debug, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub enum ContactTypes {
    Email,
    Telegram,
    Twitter,
    GovForum,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct Contact {
    pub contact_type: ContactTypes,
    pub value: String,
}

#[near_bindgen]
impl NearAuth {
    #[payable]
    pub fn start_auth(&mut self, public_key: Base58PublicKey, contact: Contact) -> Promise {
        assert!(
            env::attached_deposit() >= ACCESS_KEY_ALLOWANCE,
            "Attached deposit must be greater than ACCESS_KEY_ALLOWANCE"
        );
        let pk = public_key.into();

        self.requests.insert(
            &env::predecessor_account_id(),
            &contact,
        );

        env::log(format!("@{} add key", env::current_account_id()).as_bytes());

        Promise::new(env::current_account_id()).add_access_key(
            pk,
            ACCESS_KEY_ALLOWANCE,
            env::current_account_id(),
            b"confirm_auth".to_vec(),
        )
    }

    pub fn confirm_auth(
        &mut self,
        account_id: AccountId,
        contact: Contact,
    ) {
        assert_eq!(
            env::predecessor_account_id(),
            env::current_account_id(),
            "Auth can come from this account"
        );

        let requested_contact: Contact = self.requests.get(&account_id).unwrap();

        assert_eq!(contact.value, requested_contact.value, "Different contact data");
        assert_eq!(contact.contact_type, requested_contact.contact_type, "Different contact data");

        Promise::new(env::current_account_id()).delete_key(
            env::signer_account_pk()
        );

        //env::log(format!("{:?} key deleted", env::signer_account_pk()).as_bytes());

        let mut contacts = self.accounts.get(&account_id).unwrap_or(vec![]);
        contacts.push(contact);
        self.accounts.insert(&account_id, &contacts);

        self.requests.remove(&account_id).expect("Unexpected request");
    }

    pub fn get_request(&self, account_id: AccountId) -> Contact {
        self.requests.get(&account_id).expect("Request not found")
    }

    pub fn get_contacts(&self, account_id: AccountId) -> Vec<Contact> {
        self.accounts.get(&account_id).expect("Contacts not found")
    }
}

/*
 * The rest of this file holds the inline tests for the code above
 * Learn more about Rust tests: https://doc.rust-lang.org/book/ch11-01-writing-tests.html
 *
 * To run from contract directory:
 * cargo test -- --nocapture
 *
 * From project root, to run in combination with frontend tests:
 * yarn test
 *
 */
#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::MockedBlockchain;
    use near_sdk::{testing_env, VMContext};

    // mock the context for testing, notice "signer_account_id" that was accessed above from env::
    fn get_context(input: Vec<u8>, is_view: bool) -> VMContext {
        VMContext {
            current_account_id: "alice_near".to_string(),
            signer_account_id: "bob_near".to_string(),
            signer_account_pk: vec![0, 1, 2],
            predecessor_account_id: "carol_near".to_string(),
            input,
            block_index: 0,
            block_timestamp: 0,
            account_balance: 0,
            account_locked_balance: 0,
            storage_usage: 0,
            attached_deposit: 0,
            prepaid_gas: 10u64.pow(18),
            random_seed: vec![0, 1, 2],
            is_view,
            output_data_receivers: vec![],
            epoch_height: 19,
        }
    }

    #[test]
    fn set_then_get_greeting() {
        let context = get_context(vec![], false);
        testing_env!(context);
        let mut contract = NearAuth::default();
        contract.set_greeting("howdy".to_string());
        assert_eq!(
            "howdy".to_string(),
            contract.get_greeting("bob_near".to_string())
        );
    }

    #[test]
    fn get_default_greeting() {
        let context = get_context(vec![], true);
        testing_env!(context);
        let contract = NearAuth::default();
        // this test did not call set_greeting so should return the default "Hello" greeting
        assert_eq!(
            "Hello".to_string(),
            contract.get_greeting("francis.near".to_string())
        );
    }
}
