use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use near_sdk::wee_alloc;
use near_sdk::{env, near_bindgen, AccountId, PublicKey, Balance, Promise, PanicOnDefault};
use near_sdk::json_types::{ValidAccountId, Base58PublicKey, U128};
use near_sdk::collections::{LookupMap, UnorderedMap};
use std::collections::HashMap;

/// Price per 1 byte of storage from mainnet config after `0.18` release and protocol version `42`.
/// It's 10 times lower than the genesis price.
pub const STORAGE_PRICE_PER_BYTE: Balance = 10_000_000_000_000_000_000;

const MIN_SEND_AMOUNT: u128 = 100_000_000_00_000_000_000_000; //0.01
const STORAGE_COST_PER_KEY: u128 = 1000 * STORAGE_PRICE_PER_BYTE; //0.01

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    master_account_id: AccountId,
    accounts: UnorderedMap<AccountId, Vec<Contact>>,
    requests: UnorderedMap<PublicKey, Request>,
    storage_deposits: LookupMap<AccountId, Balance>,
}

/// Helper structure to for keys of the persistent collections.
#[derive(BorshSerialize)]
pub enum StorageKey {
    Accounts,
    Requests,
    StorageDeposits,
}

#[derive(BorshSerialize, BorshDeserialize, Eq, PartialEq, Debug, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub enum ContactCategories {
    Email,
    Telegram,
    Twitter,
    Github,
    NearGovForum,
}

#[derive(Clone, BorshDeserialize, BorshSerialize, Serialize, Deserialize, Eq, PartialEq)]
#[serde(crate = "near_sdk::serde")]
pub struct Contact {
    pub category: ContactCategories,
    pub value: String,
}

#[derive(Clone, BorshDeserialize, BorshSerialize, Serialize, Deserialize, Eq, PartialEq)]
#[serde(crate = "near_sdk::serde")]
pub struct Request {
    pub contact: Option<Contact>,
    pub account_id: AccountId,
}


#[near_bindgen]
impl Contract {
    #[init]
    pub fn new(master_account_id: ValidAccountId) -> Self {
        Self {
            master_account_id: master_account_id.into(),
            accounts: UnorderedMap::new(StorageKey::Accounts.try_to_vec().unwrap()),
            requests: UnorderedMap::new(StorageKey::Requests.try_to_vec().unwrap()),
            storage_deposits: LookupMap::new(StorageKey::StorageDeposits.try_to_vec().unwrap())
        }
    }

    pub fn whitelist_key(&mut self, account_id: ValidAccountId, public_key: Base58PublicKey) {
        assert!(env::predecessor_account_id() == self.master_account_id, "No access");

        let storage_paid = Contract::storage_paid(self, account_id.clone());

        assert!(
            storage_paid.0 >= STORAGE_COST_PER_KEY,
            "{} requires minimum storage deposit of {}",
            account_id, STORAGE_COST_PER_KEY
        );

        let account_id_string: AccountId = account_id.into();

        match Contract::get_requested_public_key(self, account_id_string.clone()) {
            None => {
                let request = Request {
                    contact: None,
                    account_id: account_id_string.clone(),
                };

                self.requests.insert(&public_key.into(), &request);

                // update storage
                let balance: Balance = storage_paid.0 - STORAGE_COST_PER_KEY;
                self.storage_deposits.insert(&account_id_string, &balance);
            }
            Some(_) => {
                env::panic(b"Request for this account already exist. Please remove it to continue")
            }
        }
    }


    #[payable]
    pub fn start_auth(&mut self, public_key: Base58PublicKey, contact: Contact) -> Promise {
        assert_one_yocto();

        let account_id: AccountId = env::predecessor_account_id();

        let contact_owner = Contract::get_owners(self, contact.clone());
        assert!(contact_owner.is_empty(), "Contact already registered");

        match self.get_request(public_key.clone()) {
            Some(request) => {
                assert_eq!(
                    request.account_id,
                    account_id,
                    "Key whitelisted for different account"
                );

                match request.contact {
                    None => {
                        self.requests.insert(
                            &public_key.clone().into(),
                            &Request {
                                contact: Some(contact),
                                account_id,
                            },
                        );

                        let pk = public_key.into();
                        Promise::new(env::current_account_id()).add_access_key(
                            pk,
                            STORAGE_COST_PER_KEY,
                            env::current_account_id(),
                            b"confirm_auth".to_vec(),
                        )
                    }
                    Some(_) =>
                        env::panic(b"Contact already exists for this request")
                }
            }
            None => env::panic(b"Only whitelisted keys allowed")
        }
    }

    pub fn confirm_auth(&mut self) {
        assert_eq!(
            env::predecessor_account_id(),
            env::current_account_id(),
            "Confirm auth can come from this contract only"
        );

        let public_key = env::signer_account_pk();
        let public_key_string: Base58PublicKey = Base58PublicKey::try_from(public_key.clone()).unwrap();

        match Contract::get_request(self, public_key_string.clone()) {
            Some(request) => {
                match request.contact {
                    Some(requested_contact) => {
                        Promise::new(env::current_account_id()).delete_key(
                            public_key
                        );

                        let account_id: AccountId = request.account_id;

                        self.requests.remove(&public_key_string.into()).expect("Unexpected request");

                        let initial_storage_usage = env::storage_usage();

                        let mut contacts = self.get_contacts(account_id.clone()).unwrap_or(vec![]);
                        contacts.push(requested_contact);
                        self.accounts.insert(&account_id, &contacts);

                        // update storage
                        let tokens_per_entry_in_bytes = env::storage_usage() - initial_storage_usage;
                        let tokens_per_entry_storage_price: Balance = Balance::from(tokens_per_entry_in_bytes) * STORAGE_PRICE_PER_BYTE;
                        let storage_paid = Contract::storage_paid(self, ValidAccountId::try_from(account_id.clone()).unwrap());

                        assert!(
                            storage_paid.0 >= tokens_per_entry_storage_price,
                            "{} requires minimum storage of {}", account_id, tokens_per_entry_storage_price
                        );

                        let balance: Balance = storage_paid.0 + STORAGE_COST_PER_KEY - tokens_per_entry_storage_price;
                        self.storage_deposits.insert(&account_id, &balance);

                        env::log(format!("@{} spent {} yNEAR for storage", account_id, tokens_per_entry_storage_price).as_bytes());
                    }
                    None =>
                        env::panic(b"Confirm of undefined contact")
                }
            }
            None => {
                env::log(format!("Illegal confirm_auth request for key {:?}", public_key_string).as_bytes());
            }
        }
    }

    pub fn get_request(&self, public_key: Base58PublicKey) -> Option<Request> {
        match self.requests.get(&public_key.into()) {
            Some(request) => Some(request),
            None => None
        }
    }


    pub fn get_requested_public_key(&self, account_id: AccountId) -> Option<PublicKey> {
        self.requests
            .iter()
            .find_map(|(key, request)| if request.account_id == account_id { Some(key.into()) } else { None })
    }

    pub fn get_requested_public_key_wrapped(&self, account_id: AccountId) -> Option<Base58PublicKey > {
        self.requests
            .iter()
            .find_map(|(key, request)| if request.account_id == account_id { Some(Base58PublicKey::try_from(key).unwrap()) } else { None })
    }

    pub fn remove_request(&mut self) {
        let account_id = env::predecessor_account_id();

        match Contract::get_requested_public_key(self, account_id.clone()) {
            Some(public_key) => {
                self.requests.remove(&public_key.clone().into());

                Promise::new(env::current_account_id()).delete_key(
                    public_key.into()
                );

                // update storage
                let storage_paid = Contract::storage_paid(self, ValidAccountId::try_from(account_id.clone()).unwrap());
                let balance: Balance = storage_paid.0 + STORAGE_COST_PER_KEY;
                self.storage_deposits.insert(&account_id, &balance);
            }
            None => {
                env::panic(b"Request not found")
            }
        }
    }


    pub fn get_contacts(&self, account_id: AccountId) -> Option<Vec<Contact>> {
        match self.accounts.get(&account_id) {
            Some(contacts) => Some(contacts),
            None => None
        }
    }

    pub fn get_contacts_by_type(&self, account_id: AccountId, category: ContactCategories) -> Option<Vec<String>> {
        match self.accounts.get(&account_id) {
            Some(contacts) =>
                {
                    let filtered_contacts: Vec<String> = contacts
                        .into_iter()
                        .filter(|contact| contact.category == category)
                        .map(|contact| contact.value)
                        .collect();
                    Some(filtered_contacts)
                }
            None => None
        }
    }

    pub fn has_requested_public_key(&self, account_id: AccountId) -> bool {
        self.get_requested_public_key(account_id) != None
    }


    #[payable]
    pub fn send(&mut self, contact: Contact) -> Promise {
        let tokens: Balance = near_sdk::env::attached_deposit();
        assert!(tokens >= MIN_SEND_AMOUNT, "Minimal amount is 0.01 NEAR");

        let owners = self.get_owners(contact);
        let owners_quantity = owners.len();
        assert!(owners_quantity > 0, "Contact not found");
        assert!(owners_quantity == 1, "Illegal contact owners quantity");

        let recipient = owners.get(0).unwrap().to_string();
        env::log(format!("Tokens sent to @{}", recipient).as_bytes());

        Promise::new(recipient).transfer(tokens)
    }


    pub fn get_all_contacts(&self, from_index: u64, limit: u64) -> HashMap<AccountId, Vec<Contact>> {
        assert!(limit <= 100, "Abort. Limit > 100");

        let keys = self.accounts.keys_as_vector();

        (from_index..std::cmp::min(from_index + limit, keys.len()))
            .map(|index| {
                let account_id = keys.get(index).unwrap();
                let all_contacts = self.get_contacts(account_id.clone()).unwrap();
                (account_id, all_contacts)
            })
            .collect()
    }

    pub fn get_all_contacts_by_type(&self, category: ContactCategories, from_index: u64, limit: u64) -> HashMap<AccountId, Vec<String>> {
        assert!(limit <= 100, "Abort. Limit > 100");

        let keys = self.accounts.keys_as_vector();

        (from_index..std::cmp::min(from_index + limit, keys.len()))
            .map(|index| {
                let account_id = keys.get(index).unwrap();
                let all_contacts = self.get_contacts_by_type(account_id.clone(), category.clone()).unwrap();
                (account_id, all_contacts)
            })
            .filter(|(_k, v)| !v.is_empty())
            .map(|(k, v)| (k, v))
            .collect()
    }

    pub fn get_owners(&self, contact: Contact) -> Vec<String> {
        let keys = self.accounts.keys_as_vector();

        (0..keys.len())
            .filter(|index| self.get_contacts(keys.get(*index).unwrap()).unwrap().contains(&contact))
            .map(|index| keys.get(index).unwrap())
            .collect()
    }

        pub fn is_owner(&self, account_id: AccountId, contact: Contact) -> bool {
        match self.accounts.get(&account_id) {
            Some(contacts) =>
                {
                    let filtered_contacts: Vec<Contact> = contacts
                        .into_iter()
                        .filter(|_contact| _contact.category == contact.category && _contact.value == contact.value)
                        // todo shorten
                        .collect();
                    filtered_contacts.len() == 1
                }
            None => false
        }
    }

    pub fn remove(&mut self, contact: Contact) -> bool {
        let account_id = env::predecessor_account_id();
        let is_owner = Contract::is_owner(self, account_id.clone(), contact.clone());

        assert!(is_owner, "Not an owner of this contact");

        match self.accounts.get(&account_id) {
            Some(contacts) =>
                {
                    let initial_storage_usage = env::storage_usage();

                    let filtered_contacts: Vec<Contact> = contacts
                        .into_iter()
                        .filter(|_contact| _contact.category != contact.category && _contact.value != contact.value)
                        .collect();
                    self.accounts.insert(&account_id, &filtered_contacts);

                    let tokens_per_entry_in_bytes = initial_storage_usage - env::storage_usage();
                    let tokens_per_entry_storage_price: Balance = Balance::from(tokens_per_entry_in_bytes) * STORAGE_PRICE_PER_BYTE;
                    let storage_paid = Contract::storage_paid(self, ValidAccountId::try_from(account_id.clone()).unwrap());
                    let balance: Balance = storage_paid.0 + tokens_per_entry_storage_price;
                    self.storage_deposits.insert(&account_id.clone(), &balance);
                    env::log(format!("@{} unlocked {} yNEAR from storage", account_id, tokens_per_entry_storage_price).as_bytes());

                    true
                }
            None => false
        }
    }

    pub fn remove_all(&mut self) {
        let account_id = env::predecessor_account_id();

        let initial_storage_usage = env::storage_usage();

        self.accounts.insert(&account_id, &vec![]);

        let tokens_per_entry_in_bytes = initial_storage_usage - env::storage_usage();
        let tokens_per_entry_storage_price: Balance = Balance::from(tokens_per_entry_in_bytes) * STORAGE_PRICE_PER_BYTE;
        let storage_paid = Contract::storage_paid(self, ValidAccountId::try_from(account_id.clone()).unwrap());
        let balance: Balance = storage_paid.0 + tokens_per_entry_storage_price;
        self.storage_deposits.insert(&account_id, &balance);
        env::log(format!("@{} unlocked {} yNEAR from storage", account_id, tokens_per_entry_storage_price).as_bytes());
    }

    #[payable]
    pub fn storage_deposit(&mut self, account_id: Option<ValidAccountId>) {
        let storage_account_id = account_id
            .map(|a| a.into())
            .unwrap_or_else(env::predecessor_account_id);
        let deposit = env::attached_deposit();
        assert!(
            deposit >= STORAGE_COST_PER_KEY,
            "Requires minimum deposit of {}",
            STORAGE_COST_PER_KEY
        );

        // update storage
        let mut balance: u128 = self.storage_deposits.get(&storage_account_id).unwrap_or(0);
        balance += deposit;
        self.storage_deposits.insert(&storage_account_id, &balance);
    }

    #[payable]
    pub fn storage_withdraw(&mut self) {
        assert_one_yocto();
        let owner_id = env::predecessor_account_id();
        let amount = self.storage_deposits.remove(&owner_id).unwrap_or(0);
        if amount > 0 {
            Promise::new(owner_id).transfer(amount);
        }
    }

    pub fn storage_amount(&self) -> U128 {
        U128(STORAGE_COST_PER_KEY)
    }

    pub fn storage_paid(&self, account_id: ValidAccountId) -> U128 {
        U128(self.storage_deposits.get(account_id.as_ref()).unwrap_or(0))
    }
}

/* UTILS */
pub(crate) fn assert_one_yocto() {
    assert_eq!(
        env::attached_deposit(),
        1,
        "Requires attached deposit of exactly 1 yoctoNEAR",
    )
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
        let mut contract = Contract::default();
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
        let contract = Contract::default();
        // this test did not call set_greeting so should return the default "Hello" greeting
        assert_eq!(
            "Hello".to_string(),
            contract.get_greeting("francis.near".to_string())
        );
    }
}
