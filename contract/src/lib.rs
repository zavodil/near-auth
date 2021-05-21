use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use near_sdk::wee_alloc;
use near_sdk::{env, near_bindgen, AccountId, Balance, Promise, PanicOnDefault};
use near_sdk::json_types::{ValidAccountId, U128};
use near_sdk::collections::{LookupMap, UnorderedMap};
use std::collections::HashMap;
use sha256::digest;

type SecretKey = String;
type RequestKey = String;

/// Price per 1 byte of storage from mainnet config after `0.18` release and protocol version `42`.
/// It's 10 times lower than the genesis price.
pub const STORAGE_PRICE_PER_BYTE: Balance = 10_000_000_000_000_000_000;
const WHITELIST_STORAGE_COST: u128 = 10_000_000_000_000_000_000_000; //0.01
const WHITELIST_FEE: u128 = 1_500_000_000_000_000_000_000; //0.0015

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    master_account_id: AccountId,
    accounts: UnorderedMap<AccountId, Vec<Contact>>,
    requests: UnorderedMap<RequestKey, Request>,
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
            storage_deposits: LookupMap::new(StorageKey::StorageDeposits.try_to_vec().unwrap()),
        }
    }

    pub fn whitelist_key(&mut self, account_id: ValidAccountId, request_key: RequestKey) {
        assert!(env::predecessor_account_id() == self.master_account_id, "No access");

        let storage_paid = Contract::storage_paid(self, account_id.clone());

        assert!(
            storage_paid.0 >= WHITELIST_STORAGE_COST,
            "{} requires minimum storage deposit of {}",
            account_id, WHITELIST_STORAGE_COST
        );

        let account_id_string: AccountId = account_id.into();

        match Contract::get_request_key(self, account_id_string.clone()) {
            None => {
                let request = Request {
                    contact: None,
                    account_id: account_id_string.clone(),
                };

                self.requests.insert(&request_key, &request);

                // update storage
                let balance: Balance = storage_paid.0 - WHITELIST_STORAGE_COST;
                self.storage_deposits.insert(&account_id_string, &balance);
            }
            Some(_) => {
                env::panic(b"Request for this account already exist. Please remove it to continue")
            }
        }
    }


    #[payable]
    pub fn start_auth(&mut self, request_key: RequestKey, contact: Contact) {
        assert_one_yocto();

        let account_id: AccountId = env::predecessor_account_id();

        let contact_owner = Contract::get_owners(self, contact.clone());
        assert!(contact_owner.is_empty(), "Contact already registered");

        match self.get_request(request_key.clone()) {
            Some(request) => {
                assert_eq!(
                    request.account_id,
                    account_id,
                    "Key whitelisted for different account"
                );

                match request.contact {
                    None => {
                        self.requests.insert(
                            &request_key,
                            &Request {
                                contact: Some(contact),
                                account_id,
                            },
                        );
                    }
                    Some(_) =>
                        env::panic(b"Contact already exists for this request")
                }
            }
            None => env::panic(b"Only whitelisted keys allowed")
        }
    }

    fn get_sha256(key: SecretKey) -> String {
        digest(key)
    }

    pub fn confirm_auth(&mut self, key: SecretKey) {
        let account_id = env::predecessor_account_id();
        let request_key = Contract::get_sha256(key);

        match Contract::get_request(self, request_key.clone()) {
            Some(request) => {
                assert_eq!(
                    account_id,
                    request.account_id,
                    "No access to confirm this request"
                );

                match request.contact {
                    Some(requested_contact) => {
                        self.requests.remove(&request_key).expect("Unexpected request");

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

                        let balance: Balance = storage_paid.0 + WHITELIST_STORAGE_COST - WHITELIST_FEE - tokens_per_entry_storage_price;
                        self.storage_deposits.insert(&account_id, &balance);

                        env::log(format!("@{} spent {} yNEAR for storage", account_id, tokens_per_entry_storage_price).as_bytes());
                    }
                    None =>
                        env::panic(b"Confirm of undefined contact")
                }
            }
            None => {
                env::panic(b"Request not found");
            }
        }
    }

    pub fn get_request(&self, request_key: RequestKey) -> Option<Request> {
        match self.requests.get(&request_key) {
            Some(request) => Some(request),
            None => None
        }
    }


    pub fn get_request_key(&self, account_id: AccountId) -> Option<RequestKey> {
        self.requests
            .iter()
            .find_map(|(key, request)| if request.account_id == account_id { Some(key) } else { None })
    }

    pub fn remove_request(&mut self) {
        let account_id = env::predecessor_account_id();

        match Contract::get_request_key(self, account_id.clone()) {
            Some(request_key) => {
                self.requests.remove(&request_key);

                // update storage
                let storage_paid = Contract::storage_paid(self, ValidAccountId::try_from(account_id.clone()).unwrap());
                let balance: Balance = storage_paid.0 + WHITELIST_STORAGE_COST - WHITELIST_FEE;
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

    pub fn has_request_key(&self, account_id: AccountId) -> bool {
        self.get_request_key(account_id) != None
    }


    #[payable]
    pub fn send(&mut self, contact: Contact) -> Promise {
        let tokens: Balance = near_sdk::env::attached_deposit();

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
            Some(contacts) => contacts.contains(&contact),
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
            deposit > 0,
            "Requires positive deposit"
        );

        // update storage
        let mut balance: u128 = self.storage_deposits.get(&storage_account_id).unwrap_or(0);
        balance += deposit;
        self.storage_deposits.insert(&storage_account_id, &balance);
    }

    #[payable]
    pub fn storage_withdraw(&mut self) {
        let owner_id = env::predecessor_account_id();
        let amount = self.storage_deposits.remove(&owner_id).unwrap_or(0);
        if amount > 0 {
            Promise::new(owner_id).transfer(amount);
        }
    }

    pub fn storage_amount(&self) -> U128 {
        U128(WHITELIST_STORAGE_COST)
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

    fn master_account() -> AccountId { "admin.near".to_string() }

    fn master_valid_account() -> ValidAccountId { ValidAccountId::try_from(master_account()).unwrap() }

    fn alice_account() -> AccountId { "alice.near".to_string() }

    fn alice_valid_account() -> ValidAccountId { ValidAccountId::try_from(alice_account()).unwrap() }

    fn bob_account() -> AccountId { "bob.near".to_string() }

    fn bob_valid_account() -> ValidAccountId { ValidAccountId::try_from(bob_account()).unwrap() }

    fn alice_request_key() -> RequestKey { digest(alice_secret_key()).to_string() }

    fn alice_secret_key() -> SecretKey { "be1AcEnEsBVV4UuoZ6qGGHRFK3HDwckDj7pctw83BbkR7JJsQLs7y1gbv78f1o7UkqFAHX45CA82UPT7kDdBaSL".to_string() }

    fn bob_request_key() -> RequestKey { digest(bob_secret_key()).to_string() }

    fn bob_secret_key() -> SecretKey { "WRONG_KEY_be1AcEnEsBVV4UuoZ6qGGHRFK3HDwckDj7pctw83BbkR7JJsQLs7y1gbv78f1o7UkqFAHX45CA82U".to_string() }

    fn alice_contact() -> Contact {
        Contact {
            category: ContactCategories::Telegram,
            value: "123".to_string(),
        }
    }

    fn bob_contact() -> Contact {
        Contact {
            category: ContactCategories::Telegram,
            value: "456".to_string(),
        }
    }


    pub fn get_context(
        predecessor_account_id: AccountId,
        attached_deposit: u128,
        is_view: bool,
    ) -> VMContext {
        VMContext {
            current_account_id: predecessor_account_id.clone(),
            signer_account_id: predecessor_account_id.clone(),
            signer_account_pk: vec![0, 1, 2],
            predecessor_account_id,
            input: vec![],
            block_index: 1,
            block_timestamp: 0,
            epoch_height: 1,
            account_balance: 0,
            account_locked_balance: 0,
            storage_usage: 10u64.pow(6),
            attached_deposit,
            prepaid_gas: 10u64.pow(15),
            random_seed: vec![0, 1, 2],
            is_view,
            output_data_receivers: vec![],
        }
    }

    fn ntoy(near_amount: Balance) -> Balance {
        near_amount * 10u128.pow(24)
    }

    #[test]
    fn test_storage_deposit() {
        let context = get_context(alice_account(), ntoy(100), false);
        testing_env!(context.clone());

        let mut contract = Contract::new(master_valid_account());

        contract.storage_deposit(Some(alice_valid_account()));

        assert_eq!(
            ntoy(100),
            contract.storage_paid(alice_valid_account()).0
        );
    }

    #[test]
    fn test_storage_deposit_and_withdraw() {
        let context = get_context(alice_account(), ntoy(100), false);
        testing_env!(context.clone());

        let mut contract = Contract::new(master_valid_account());

        contract.storage_deposit(Some(alice_valid_account()));
        contract.storage_withdraw();

        assert_eq!(
            0,
            contract.storage_paid(alice_valid_account()).0
        );
    }

    #[test]
    fn whitelist_key() {
        let context = get_context(alice_account(), ntoy(100), false);
        testing_env!(context.clone());

        let mut contract = Contract::new(master_valid_account());

        contract.storage_deposit(Some(alice_valid_account()));

        let storage_paid_before = contract.storage_paid(alice_valid_account()).0;

        // switch to a context with master_account
        let context = get_context(master_account(), 0, false);
        testing_env!(context.clone());
        contract.whitelist_key(alice_valid_account(), alice_request_key());

        let storage_paid_after = contract.storage_paid(alice_valid_account()).0;
        assert!(storage_paid_before == storage_paid_after + WHITELIST_STORAGE_COST,
                "Wrong storage deposit for whitelist {} / {}", storage_paid_before, storage_paid_after);

        let alice_key = contract.get_request_key(alice_account());
        assert_eq!(alice_key, Some(alice_request_key()), "Key wasn't added");

        let bob_key = contract.get_request_key(bob_account());
        assert!(bob_key != Some(bob_request_key()), "Wrong key added");

        let alice_has_key = contract.has_request_key(alice_account());
        assert_eq!(alice_has_key, true, "Key wasn't added");

        let bob_has_key = contract.has_request_key(bob_account());
        assert_eq!(bob_has_key, false, "Wrong key added");

        let request: Request = contract.get_request(alice_request_key()).unwrap();
        assert_eq!(request.account_id, alice_account(), "Key wasn't added");
        assert!(request.account_id != bob_account(), "Wrong key added");
        assert!(request.contact == None, "Contact not empty");
    }

    #[test]
    #[should_panic(expected = "No access")]
    fn whitelist_by_user() {
        let context = get_context(alice_account(), ntoy(100), false);
        testing_env!(context.clone());

        let mut contract = Contract::new(master_valid_account());

        contract.storage_deposit(Some(alice_valid_account()));

        contract.whitelist_key(alice_valid_account(), bob_request_key());
    }

    #[test]
    #[should_panic(expected = "Request for this account already exist. Please remove it to continue")]
    fn whitelist_twice() {
        let context = get_context(alice_account(), ntoy(100), false);
        testing_env!(context.clone());

        let mut contract = Contract::new(master_valid_account());

        contract.storage_deposit(Some(alice_valid_account()));

        // switch to a context with master_account
        let context = get_context(master_account(), 0, false);
        testing_env!(context.clone());
        contract.whitelist_key(alice_valid_account(), alice_request_key());

        contract.whitelist_key(alice_valid_account(), bob_request_key());
    }

    #[test]
    #[should_panic(expected = "alice.near requires minimum storage deposit of 10000000000000000000000")]
    fn whitelist_without_storage() {
        let context = get_context(alice_account(), ntoy(1) / 1000, false);
        testing_env!(context.clone());

        let mut contract = Contract::new(master_valid_account());

        contract.storage_deposit(Some(alice_valid_account()));

        // switch to a context with master_account
        let context = get_context(master_account(), 0, false);
        testing_env!(context.clone());
        contract.whitelist_key(alice_valid_account(), alice_request_key());
    }

    #[test]
    fn remove_request_after_whitelist() {
        let context = get_context(alice_account(), ntoy(100), false);
        testing_env!(context.clone());

        let mut contract = Contract::new(master_valid_account());

        contract.storage_deposit(Some(alice_valid_account()));
        let storage_paid_before = contract.storage_paid(alice_valid_account()).0;

        // switch to a context with master_account
        let context = get_context(master_account(), 0, false);
        testing_env!(context.clone());
        contract.whitelist_key(alice_valid_account(), alice_request_key());

        // switch back to a context with user
        let context = get_context(alice_account(), 0, false);
        testing_env!(context.clone());

        contract.remove_request();

        let request: Option<Request> = contract.get_request(alice_request_key());
        assert!(request == None, "Request was not removed");

        let storage_paid_after = contract.storage_paid(alice_valid_account()).0;
        assert!(storage_paid_before == storage_paid_after,
                "Wrong storage deposit for remove_request {} / {}", storage_paid_before, storage_paid_after);

        // switch to a context with master_account
        let context = get_context(master_account(), 0, false);
        testing_env!(context.clone());
        contract.whitelist_key(alice_valid_account(), alice_request_key());

        let alice_has_key = contract.has_request_key(alice_account());
        assert_eq!(alice_has_key, true, "Key wasn't added on a second time");
    }

    #[test]
    fn start_auth() {
        let context = get_context(alice_account(), ntoy(100), false);
        testing_env!(context.clone());

        let mut contract = Contract::new(master_valid_account());

        contract.storage_deposit(Some(alice_valid_account()));

        // switch to a context with master_account
        let context = get_context(master_account(), 0, false);
        testing_env!(context.clone());
        contract.whitelist_key(alice_valid_account(), alice_request_key());

        // switch back to a context with user
        let context = get_context(alice_account(), 1, false);
        testing_env!(context.clone());

        contract.start_auth(alice_request_key(), alice_contact());

        let request: Request = contract.get_request(alice_request_key()).unwrap();
        assert!(request.contact == Some(alice_contact()), "Contact wasn't properly saved");
    }

    #[test]
    fn remove_request_after_start_auth() {
        let context = get_context(alice_account(), ntoy(100), false);
        testing_env!(context.clone());

        let mut contract = Contract::new(master_valid_account());

        contract.storage_deposit(Some(alice_valid_account()));
        let storage_paid_before = contract.storage_paid(alice_valid_account()).0;

        // switch to a context with master_account
        let context = get_context(master_account(), 0, false);
        testing_env!(context.clone());
        contract.whitelist_key(alice_valid_account(), alice_request_key());

        // switch back to a context with user
        let context = get_context(alice_account(), 1, false);
        testing_env!(context.clone());

        contract.start_auth(alice_request_key(), alice_contact());

        contract.remove_request();

        let request: Option<Request> = contract.get_request(alice_request_key());
        assert!(request == None, "Request was not removed");

        let storage_paid_after = contract.storage_paid(alice_valid_account()).0;
        assert!(storage_paid_before == storage_paid_after,
                "Wrong storage deposit for remove_request {} / {}", storage_paid_before, storage_paid_after);
    }

    #[test]
    fn confirm_auth() {
        let context = get_context(alice_account(), ntoy(100), false);
        testing_env!(context.clone());

        let mut contract = Contract::new(master_valid_account());

        contract.storage_deposit(Some(alice_valid_account()));
        let storage_paid_before = contract.storage_paid(alice_valid_account()).0;

        // switch to a context with master_account
        let context = get_context(master_account(), 0, false);
        testing_env!(context.clone());
        contract.whitelist_key(alice_valid_account(), alice_request_key());

        // switch back to a context with user
        let context = get_context(alice_account(), 1, false);
        testing_env!(context.clone());

        contract.start_auth(alice_request_key(), alice_contact());

        let secret_key: SecretKey = digest(alice_secret_key());
        assert!(secret_key == "9f763044a36137644ca87a50545c3eff219345d8490d1c1db597105411315a9a", "Wrong secret key generation");
        contract.confirm_auth(alice_secret_key());

        let alice_is_owner = contract.is_owner(alice_account(), alice_contact());
        assert!(alice_is_owner == true, "Contact wasn't created");

        let bob_is_owner = contract.is_owner(bob_account(), alice_contact());
        assert!(bob_is_owner == false, "Wrong contact owner");

        let storage_paid_after = contract.storage_paid(alice_valid_account()).0;
        assert!(storage_paid_before > storage_paid_after,
                "Storage deposit wasn't reduced after adding an item {} / {}", storage_paid_before, storage_paid_after);
    }

    #[test]
    #[should_panic(expected = "Request not found")]
    fn confirm_auth_with_wrong_key() {
        let context = get_context(alice_account(), ntoy(100), false);
        testing_env!(context.clone());

        let mut contract = Contract::new(master_valid_account());

        contract.storage_deposit(Some(alice_valid_account()));

        // switch to a context with master_account
        let context = get_context(master_account(), 0, false);
        testing_env!(context.clone());
        contract.whitelist_key(alice_valid_account(), alice_request_key());

        // switch back to a context with user
        let context = get_context(alice_account(), 1, false);
        testing_env!(context.clone());

        contract.start_auth(alice_request_key(), alice_contact());

        contract.confirm_auth(bob_secret_key());
    }

    #[test]
    #[should_panic(expected = "No access to confirm this request")]
    fn confirm_auth_with_wrong_user() {
        let context = get_context(alice_account(), ntoy(100), false);
        testing_env!(context.clone());

        let mut contract = Contract::new(master_valid_account());

        contract.storage_deposit(Some(alice_valid_account()));

        // switch to a context with master_account
        let context = get_context(master_account(), 0, false);
        testing_env!(context.clone());
        contract.whitelist_key(alice_valid_account(), alice_request_key());

        // switch back to a context with user
        let context = get_context(alice_account(), 1, false);
        testing_env!(context.clone());

        contract.start_auth(alice_request_key(), alice_contact());

        // switch back to a context with user
        let context = get_context(bob_account(), 1, false);
        testing_env!(context.clone());

        contract.confirm_auth(alice_secret_key());
    }

    #[test]
    #[should_panic(expected = "Contact already registered")]
    fn add_same_contact_twice() {
        let context = get_context(alice_account(), ntoy(100), false);
        testing_env!(context.clone());

        let mut contract = Contract::new(master_valid_account());

        contract.storage_deposit(Some(alice_valid_account()));

        // switch to a context with master_account
        let context = get_context(master_account(), 0, false);
        testing_env!(context.clone());
        contract.whitelist_key(alice_valid_account(), alice_request_key());

        // switch back to a context with user
        let context = get_context(alice_account(), 1, false);
        testing_env!(context.clone());

        contract.start_auth(alice_request_key(), alice_contact());
        contract.confirm_auth(alice_secret_key());

        // switch to bob

        let context = get_context(bob_account(), ntoy(100), false);
        testing_env!(context.clone());
        contract.storage_deposit(Some(bob_valid_account()));

        // switch to a context with master_account
        let context = get_context(master_account(), 0, false);
        testing_env!(context.clone());
        contract.whitelist_key(bob_valid_account(), bob_request_key());

        // switch back to a context with user
        let context = get_context(alice_account(), 1, false);
        testing_env!(context.clone());

        contract.start_auth(alice_request_key(), alice_contact());
        contract.confirm_auth(bob_secret_key());
    }

    #[test]
    fn remove_contact() {
        let context = get_context(alice_account(), ntoy(100), false);
        testing_env!(context.clone());

        let mut contract = Contract::new(master_valid_account());

        contract.storage_deposit(Some(alice_valid_account()));

        // switch to a context with master_account
        let context = get_context(master_account(), 0, false);
        testing_env!(context.clone());
        contract.whitelist_key(alice_valid_account(), alice_request_key());

        // switch back to a context with user
        let context = get_context(alice_account(), 1, false);
        testing_env!(context.clone());

        contract.start_auth(alice_request_key(), alice_contact());
        contract.confirm_auth(alice_secret_key());

        assert!(contract.is_owner(alice_account(), alice_contact()) == true, "Contact wasn't created");

        contract.remove(alice_contact());

        assert!(contract.is_owner(alice_account(), alice_contact()) == false, "Contact wasn't removed");
    }

    #[test]
    #[should_panic(expected = "Contact not found")]
    fn send_to_contact() {
        let context = get_context(alice_account(), ntoy(100), false);
        testing_env!(context.clone());

        let mut contract = Contract::new(master_valid_account());

        contract.storage_deposit(Some(alice_valid_account()));

        // switch to a context with master_account
        let context = get_context(master_account(), 0, false);
        testing_env!(context.clone());
        contract.whitelist_key(alice_valid_account(), alice_request_key());

        // switch back to a context with user
        let context = get_context(alice_account(), 1, false);
        testing_env!(context.clone());

        contract.start_auth(alice_request_key(), alice_contact());
        contract.confirm_auth(alice_secret_key());

        // send from bob

        let context = get_context(bob_account(), ntoy(75), false);
        testing_env!(context.clone());
        contract.send(alice_contact());

        let context = get_context(bob_account(), ntoy(75), false);
        testing_env!(context.clone());
        contract.send(bob_contact());
    }
}
