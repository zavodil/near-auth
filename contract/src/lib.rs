use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::wee_alloc;
use near_sdk::{env, near_bindgen, AccountId, Balance, Promise};
use near_sdk::json_types::Base58PublicKey;
use near_sdk::collections::UnorderedMap;

const ACCESS_KEY_ALLOWANCE: u128 = 10_000_000_000_000_000_000_000;
// 0.01
const MIN_DEPOSIT_AMOUNT: u128 = 100_000_000_000_000_000_000_000;
// 0.1
const MASTER_ACCOUNT_ID: &str = "dev-1614425625888-4173456"; // account to whitelist keys
// TODO Set master account

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize)]
pub struct NearAuth {
    accounts: UnorderedMap<AccountId, Vec<Contact>>,
    requests: UnorderedMap<AccountId, Contact>,
    whitelisted_keys: UnorderedMap<AccountId, Base58PublicKey>,
}

impl Default for NearAuth {
    fn default() -> Self {
        Self {
            accounts: UnorderedMap::new(b"e".to_vec()),
            requests: UnorderedMap::new(b"p".to_vec()),
            whitelisted_keys: UnorderedMap::new(b"k".to_vec()),
        }
    }
}

#[derive(BorshSerialize, BorshDeserialize, Eq, PartialEq, Debug, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub enum ContactTypes {
    Email,
    Telegram,
    Twitter,
    Github,
    NearGovForum,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Eq, PartialEq)]
#[serde(crate = "near_sdk::serde")]
pub struct Contact {
    pub contact_type: ContactTypes,
    pub value: String,
}


#[near_bindgen]
impl NearAuth {
    pub fn whitelist_key(&mut self, account_id: AccountId, public_key: Base58PublicKey) {
        assert!(env::predecessor_account_id() == MASTER_ACCOUNT_ID, "No access");
        self.whitelisted_keys.insert(&account_id, &public_key);
    }

    #[payable]
    pub fn start_auth(&mut self, public_key: Base58PublicKey, contact: Contact) -> Promise {
        assert!(
            env::attached_deposit() >= ACCESS_KEY_ALLOWANCE,
            "Attached deposit must be greater than ACCESS_KEY_ALLOWANCE"
        );

        assert!(self.has_whitelisted_key(env::predecessor_account_id()) == true,
                "Whitelisted key not found");

        assert_eq!(
            self.get_whitelisted_key(env::predecessor_account_id()).unwrap(),
            public_key,
            "Only whitelisted keys are allowed"
        );

        self.requests.insert(
            &env::predecessor_account_id(),
            &contact,
        );

        env::log(format!("@{} add key", env::current_account_id()).as_bytes());

        self.whitelisted_keys.remove(&env::predecessor_account_id());

        let pk = public_key.into();

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

        let mut contacts = self.accounts.get(&account_id).unwrap_or(vec![]);
        contacts.push(contact);
        self.accounts.insert(&account_id, &contacts);

        self.requests.remove(&account_id).expect("Unexpected request");
    }

    pub fn get_request(&self, account_id: AccountId) -> Option<Contact> {
        match self.requests.get(&account_id) {
            Some(contact) => Some(contact),
            None => None
        }
    }

    pub fn get_contacts(&self, account_id: AccountId) -> Option<Vec<Contact>> {
        match self.accounts.get(&account_id) {
            Some(contacts) => Some(contacts),
            None => None
        }
    }

    pub fn get_whitelisted_key(&self, account_id: AccountId) -> Option<Base58PublicKey> {
        match self.whitelisted_keys.get(&account_id) {
            Some(key) => Some(key),
            None => None
        }
    }

    pub fn has_whitelisted_key(&self, account_id: AccountId) -> bool {
        self.get_whitelisted_key(account_id) != None
    }

    pub fn remove_whitelisted_key(&mut self) {
        self.whitelisted_keys.remove(&env::predecessor_account_id());
    }


    #[payable]
    pub fn send(&mut self, contact: Contact) -> Promise {
        let tokens: Balance = near_sdk::env::attached_deposit();
        assert!(tokens >= MIN_DEPOSIT_AMOUNT, "Minimal amount is 0.1 NEAR");

        let owners = self.get_owners(contact);
        assert!(owners.len() > 0, "Contact not found");
        assert!(owners.len() == 1, "Illegal contact owners quantity");

        let recipient = owners.get(0).unwrap().to_string();
        env::log(format!("Tokens sent to @{}", recipient.clone()).as_bytes());

        Promise::new(recipient).transfer(tokens)
    }


    pub fn get_all_contacts(&self, from_index: u64, limit: u64) -> Vec<Contact> {
        let keys = self.accounts.keys_as_vector();

        (from_index..std::cmp::min(from_index + limit, keys.len()))
            .flat_map(|index| self.get_contacts(keys.get(index).unwrap()).unwrap())
            .collect()
    }

    pub fn get_owners(&self, contact: Contact) -> Vec<String> {
        let keys = self.accounts.keys_as_vector();

        (0..keys.len())
            .filter(|index| self.get_contacts(keys.get(*index).unwrap()).unwrap().contains(&contact))
            .map(|index| keys.get(index).unwrap())
            .collect()
    }
    
    // TODO add remove contact method
}

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
