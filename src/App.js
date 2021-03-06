import 'regenerator-runtime/runtime'
import React from 'react'
import GitHubLogin from './GitHubLogin';

const queryString = require('query-string');
const md5 = require('md5');
import {login, logout} from './utils'
import './global.css'
import './app.css'
import * as nearAPI from 'near-api-js'
import {BN} from 'bn.js'
import ReactTooltip from 'react-tooltip';
import Dropdown from 'react-dropdown';
import 'react-dropdown/style.css';
import {useDetectOutsideClick} from "./useDetectOutsideClick";

import getConfig from './config'
import getAppSettings from './app-settings'

const config = getConfig(process.env.NODE_ENV || 'development');
const appSettings = getAppSettings();

const FRAC_DIGITS = 5;
const MIN_STORAGE_DEPOSIT = 0.02;
const REQUIRED_STORAGE_DEPOSIT = 0.015;

function ConvertToYoctoNear(amount) {
    return new BN(Math.round(amount * 100000000)).mul(new BN("10000000000000000")).toString();
}

export default function App() {
    // use React Hooks to store greeting in component state
    const [contactType, setContactType] = React.useState("Telegram");
    const [warning, setWarning] = React.useState("");
    const [complete, setComplete] = React.useState("");
    const [contacts, setContacts] = React.useState([]);
    const [userPicture, setUserPicture] = React.useState("");
    const [whiteListedKeyRemove, setWhiteListedKeyRemove] = React.useState(false)
    const [isInputContactFormAvailable, setIsInputContactFormAvailable] = React.useState(true);
    const [isGithubLoginAvailable, setIsGithubLoginAvailable] = React.useState(false);

    const [showSendForm, setShowSendForm] = React.useState(false);
    const [sendFormNearAmount, setSendFormNearAmount] = React.useState(0);
    const [sendFormType, setSendFormType] = React.useState("");
    const [sendFormContact, setSendFormContact] = React.useState("");
    const [sendFormResult, setSendFormResult] = React.useState("");

    const [showStorageDepositMenuClick, setShowStorageDepositMenuClick] = React.useState(false);
    const [hideStorageDeposit, setHideStorageDeposit] = React.useState(false);
    const [storagePaid, setStoragePaid] = React.useState(0);
    const [inputStorageDeposit, setInputStorageDeposit] = React.useState(MIN_STORAGE_DEPOSIT);
    const [incufficientStorage, setIncufficientStorage] = React.useState(true);

    // when the user has not yet interacted with the form, disable the button
    const [buttonDisabled, setButtonDisabled] = React.useState(true)

    const navDropdownRef = React.useRef(null);
    const [isNavDropdownActive, setIsNaVDropdownActive] = useDetectOutsideClick(navDropdownRef, false);

    // after submitting the form, we want to show Notification
    const [showNotification, setShowNotification] = React.useState(false)

    const Warning = () => {
        return (
            !warning ? null :
                <div className="warning" dangerouslySetInnerHTML={{__html: warning}}/>)
    };

    const Complete = () => {
        return (
            !complete ? null :
                <div className="complete" dangerouslySetInnerHTML={{__html: complete}}/>)
    };

    //const FULL_GAS = 300000000000000;
    const FULL_GAS = 200000000000000;

    const WhiteListedKeyRemove = () => {
        return (
            !whiteListedKeyRemove ? null :
                <div>
                    <button
                        className="abort-previous-button"
                        onClick={async event => {
                            event.preventDefault()
                            try {
                                await window.contract.remove_request({}, FULL_GAS);
                                setWhiteListedKeyRemove(false);
                            } catch (e) {
                                alert(
                                    'Something went wrong! \n' +
                                    'Check your browser console for more info.\n' +
                                    e.toString()
                                )
                                throw e
                            }

                            setShowNotification({method: "call", data: "remove_request"});
                            setTimeout(() => {
                                setShowNotification("")
                            }, 11000)
                        }}
                    >
                        Abort previous auth attempt
                    </button>
                </div>
        )
    };

    function timeout(ms) {
        return new Promise(resolve => setTimeout(resolve, ms));
    }

    async function sleep(fn, ...args) {
        await timeout(3000);
        return fn(...args);
    }

    const OnGithubLoginSuccess = async (response) => {
        let goOn = response && response.hasOwnProperty("code");
        if (goOn) {
            setComplete("Github auth request found. Processing... ");

            while (goOn) {
                var [parents] = await Promise.all([
                    await fetch("telegram.php", {
                        method: 'POST',
                        body: JSON.stringify({
                            operation: "start",
                            account_id: window.accountId,
                            contact: response.code,
                            contact_type: contactType,
                            network: config.networkId,
                        }),
                        headers: {
                            'Accept': 'application/json',
                            'Content-Type': 'application/json'
                        }
                    })
                        .then(response => response.json())
                        .then(async data => {
                            if (data.status) {
                                goOn = false;
                                setComplete(data.text);
                                setWarning("");

                                data.category = contactType.toLowerCase()
                                window.localStorage.setItem('request', data ? JSON.stringify(data) : "[]");

                                try {
                                    await window.contract.start_auth({
                                        request_key: data.request_key,
                                        contact: {category: contactType, value: data.contact},
                                    }, FULL_GAS, 1)
                                } catch (e) {
                                    ContractCallAlert();
                                    throw e
                                }

                            } else {
                                setComplete("");
                                if (data.value === "already_has_key") {
                                    goOn = false;
                                    setWhiteListedKeyRemove(true);
                                    setWarning(data.text);
                                } else if (data.value === "contact_already_exists") {
                                    goOn = false;
                                    setWarning(data.text);
                                } else {
                                    setWarning("Auth failed. Please check console for details.");
                                    console.log(data);
                                }
                            }
                            window.history.replaceState({}, document.title, "/");
                        })
                        .catch(err => console.error("Error:", err)),

                    timeout(500)
                ]);
            }
        }
    }
    const OnGithubLoginFailure = (response) => {
        console.log("Github Login Failure");
        console.error(response);
    }

    const LoginGithub = () => {
        const url = (config.networkId === "testnet") ? appSettings.urlTestnet : appSettings.urlMainnet;
        if (isGithubLoginAvailable && incufficientStorage)
            return <button disabled={true}>Not available</button>;
        else
            return isGithubLoginAvailable
                ? <GitHubLogin clientId="4fe06f510cf9e8f40028"
                               redirectUri={`${url}github.php`}
                               scope='user:email'
                               className="github-login-button"
                               onSuccess={OnGithubLoginSuccess}
                               onFailure={OnGithubLoginFailure}/>
                : null;
    };

    const StoragePaid = () => {
        if (window.walletConnection.isSignedIn()) {
            return <div className="nav user-balance" data-tip="Your Near Auth Storage"
                        onClick={async event => {
                            setShowStorageDepositMenuClick(true)
                        }}>
                {`Storage: ${storagePaid} Ⓝ`}
            </div>;
        } else {
            return null;
        }
    };

    const Header = () => {
        return <div className="nav-container">
            <div className="nav-header">
                <NearLogo/>
                <UserPicture/>
                <div className="nav-item user-name">{window.accountId}</div>
                <StoragePaid/>
                <div className="nav align-right">
                    <NavMenu/>
                    <div className="account-sign-out">
                        <button className="link" style={{float: 'right'}} onClick={logout}>
                            Sign out
                        </button>
                    </div>
                </div>
            </div>
        </div>
    };

    const Footer = () => {
        return <div className="footer">
            <div className="github">
                <div className="build-on-near"><a href="https://nearspace.info">BUILD ON NEAR</a></div>
                <div className="brand">Near {appSettings.appNme} | <a href={appSettings.github}
                                                                      rel="nofollow"
                                                                      target="_blank">Open Source</a></div>
            </div>
            <div className="promo">
                Made by <a href="https://near.zavodil.ru/" rel="nofollow" target="_blank">Zavodil node</a>
            </div>
        </div>
    };

    const UserPicture = () => {
        return userPicture ?
            <div className="user-picture"><img
                src={"http://www.gravatar.com/avatar/" + md5(userPicture) + ".jpg?s=30"}/></div> : null;
    }

    const NearLogo = () => {
        return <div className="logo-container content-desktop">
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 414 162" className="near-logo">
                <g id="Layer_1" data-name="Layer 1">
                    <path className="polymorph"
                          d="M207.21,54.75v52.5a.76.76,0,0,1-.75.75H201a7.49,7.49,0,0,1-6.3-3.43l-24.78-38.3.85,19.13v21.85a.76.76,0,0,1-.75.75h-7.22a.76.76,0,0,1-.75-.75V54.75a.76.76,0,0,1,.75-.75h5.43a7.52,7.52,0,0,1,6.3,3.42l24.78,38.24-.77-19.06V54.75a.75.75,0,0,1,.75-.75h7.22A.76.76,0,0,1,207.21,54.75Z"
                    ></path>
                    <path className="polymorph"
                          d="M281,108h-7.64a.75.75,0,0,1-.7-1L292.9,54.72A1.14,1.14,0,0,1,294,54h9.57a1.14,1.14,0,0,1,1.05.72L324.8,107a.75.75,0,0,1-.7,1h-7.64a.76.76,0,0,1-.71-.48l-16.31-43a.75.75,0,0,0-1.41,0l-16.31,43A.76.76,0,0,1,281,108Z"
                    ></path>
                    <path className="polymorph"
                          d="M377.84,106.79,362.66,87.4c8.57-1.62,13.58-7.4,13.58-16.27,0-10.19-6.63-17.13-18.36-17.13H336.71a1.12,1.12,0,0,0-1.12,1.12h0a7.2,7.2,0,0,0,7.2,7.2H357c7.09,0,10.49,3.63,10.49,8.87s-3.32,9-10.49,9H336.71a1.13,1.13,0,0,0-1.12,1.13v26a.75.75,0,0,0,.75.75h7.22a.76.76,0,0,0,.75-.75V87.87h8.33l13.17,17.19a7.51,7.51,0,0,0,6,2.94h5.48A.75.75,0,0,0,377.84,106.79Z"
                    ></path>
                    <path className="polymorph"
                          d="M258.17,54h-33.5a1,1,0,0,0-1,1h0A7.33,7.33,0,0,0,231,62.33h27.17a.74.74,0,0,0,.75-.75V54.75A.75.75,0,0,0,258.17,54Zm0,45.67h-25a.76.76,0,0,1-.75-.75V85.38a.75.75,0,0,1,.75-.75h23.11a.75.75,0,0,0,.75-.75V77a.75.75,0,0,0-.75-.75H224.79a1.13,1.13,0,0,0-1.12,1.13v29.45a1.12,1.12,0,0,0,1.12,1.13h33.38a.75.75,0,0,0,.75-.75v-6.83A.74.74,0,0,0,258.17,99.67Z"
                    ></path>
                    <path className="polymorph"
                          d="M108.24,40.57,89.42,68.5a2,2,0,0,0,3,2.63l18.52-16a.74.74,0,0,1,1.24.56v50.29a.75.75,0,0,1-1.32.48l-56-67A9.59,9.59,0,0,0,47.54,36H45.59A9.59,9.59,0,0,0,36,45.59v70.82A9.59,9.59,0,0,0,45.59,126h0a9.59,9.59,0,0,0,8.17-4.57L72.58,93.5a2,2,0,0,0-3-2.63l-18.52,16a.74.74,0,0,1-1.24-.56V56.07a.75.75,0,0,1,1.32-.48l56,67a9.59,9.59,0,0,0,7.33,3.4h2a9.59,9.59,0,0,0,9.59-9.59V45.59A9.59,9.59,0,0,0,116.41,36h0A9.59,9.59,0,0,0,108.24,40.57Z"
                    ></path>
                </g>
            </svg>
            <div className="app-name">
                <a href="/">Near {appSettings.appNme}</a>
            </div>
        </div>;
    };

    const NavMenu = () => {
        const onClick = () => setIsNaVDropdownActive(!isNavDropdownActive);

        return (
            <div className="nav-menu container">
                <div className="menu-container">
                    <button onClick={onClick} className="menu-trigger">
                        <span className="network-title">{config.networkId}</span>
                        <div className="network-icon"></div>
                    </button>
                    <nav
                        ref={navDropdownRef}
                        className={`menu ${isNavDropdownActive ? "active" : "inactive"}`}
                    >
                        <ul>
                            <li>
                                <a href={appSettings.urlMainnet}>Mainnet</a>
                            </li>
                            <li>
                                <a href={appSettings.urlTestnet}>Testnet</a>
                            </li>
                        </ul>
                    </nav>
                </div>
            </div>
        );
    };

    const GetStoragePaid = async () => {
        const storagePaid = await window.contract.storage_paid({
            account_id: window.accountId
        });
        const storagePaidFormatted = storagePaid && Number(storagePaid)
            ? nearAPI.utils.format.formatNearAmount(storagePaid, FRAC_DIGITS).replace(",", "")
            : "0";
        setStoragePaid(storagePaidFormatted);

        setIncufficientStorage(Number(storagePaidFormatted) < 0.0101);

        return storagePaidFormatted;
    };

    React.useEffect(
        async () => {

            // in this case, we only care to query the contract when signed in
            if (window.walletConnection.isSignedIn()) {
                await GetStoragePaid().then((storagePaidFormatted) => {
                    console.log(storagePaidFormatted);
                });

                try {
                    let requestRound = false;

                    if (location.search) {
                        const query = JSON.parse(JSON.stringify(queryString.parse(location.search)));
                        if (query && query.hasOwnProperty("key") && query.hasOwnProperty("contact") && query.hasOwnProperty("type")) {
                            const request_key = await window.contract.get_request_key({
                                account_id: window.accountId
                            });

                            const has_request_key = await window.contract.has_request_key({
                                account_id: window.accountId
                            });

                            if (has_request_key && request_key) {
                                setComplete("Auth request found. Processing... ");

                                await window.contract.confirm_auth({
                                        key: query.key,
                                    }, FULL_GAS
                                )
                                    .then(data => {
                                        if (data.status) {
                                            setComplete(data.text);
                                            setWarning("");
                                        } else {
                                            setComplete("");
                                            setWarning("");
                                            //setWarning("Auth failed. Please check console for details.");
                                            console.log(data.text);
                                        }
                                        GetStoragePaid();
                                        window.history.replaceState({}, document.title, "/");
                                        requestRound = true;
                                    })
                                    .catch(err => console.error("Error:", err));
                            } else {
                                window.localStorage.setItem('request', "[]");
                                window.history.replaceState({}, document.title, "/");
                            }
                        } else if (query && query.hasOwnProperty("action")) {
                            if (query.action === "send" && query.hasOwnProperty("contact") && query.hasOwnProperty("type") && query.hasOwnProperty("amount")) {
                                const type = query.type;
                                console.log(query);
                                console.log(Number(query.amount));
                                if (dropdownOptions.includes(type)) {
                                    setShowSendForm(true);
                                    setSendFormContact(query.contact);
                                    setSendFormType(type);
                                    setSendFormNearAmount(Number(query.amount));
                                }
                            }

                        }
                    }
                    if (!requestRound)
                        await GetRequest();

                    await GetContacts();
                } catch (e) {
                    console.log(e)
                }
                // window.contract is set by initContract in index.js


            }
        },

        // The second argument to useEffect tells React when to re-run the effect
        // Use an empty array to specify "only run on first render"
        // This works because signing into NEAR Wallet reloads the page
        []
    )

    // if not signed in, return early with sign-in prompt
    if (!window.walletConnection.isSignedIn()) {
        return (
            <>
                <Header/>
                <main>
                    <h1>Near {appSettings.appNme}</h1>
                    <p>
                        {appSettings.appDescription}
                    </p>
                    <p>
                        To make use of the NEAR blockchain, you need to sign in. The button
                        below will sign you in using NEAR Wallet.
                    </p>
                    <p style={{textAlign: 'center', marginTop: '2.5em'}}>
                        <button onClick={login}>Sign in</button>
                    </p>
                </main>
                <Footer/>
            </>
        )
    }
    const dropdownOptions = [
        'Telegram', 'Email', 'Github'
    ];

    const Contacts = () => {
        let i = 0;
        return contacts.length ?
            <div className="contacts">
                <div>Your contacts:</div>
                <ul className="accounts">
                    {Object.keys(contacts).map(function (key) {
                        return <li key={contacts[key].value + "-" + i++}>
                            <div className="account">{contacts[key].value}</div>
                            <div className="type">{contacts[key].category}</div>
                            <div className="type"><RemoveContact contact={contacts[key]}/></div>
                        </li>;
                    })}
                </ul>
            </div> :
            null;
    }

    const RemoveContact = (props) => {
        return (
            <button
                type="clear"
                className="remove-contact"
                title="Remove Contact"
                onClick={async event => {
                    event.preventDefault()
                    try {
                        await window.contract.remove({
                                contact: props.contact,
                            },
                            100000000000000, 0).then(() => GetContacts());

                    } catch (e) {
                        alert(
                            'Something went wrong! \n' +
                            'Check your browser console for more info.\n' +
                            e.toString()
                        )
                        throw e
                    }


                }}
            >
                ✗
            </button>
        )
    }

    const GetContacts = async () => {
        try {
            const contacts = await window.contract.get_contacts({
                account_id: window.accountId
            });

            if (contacts) {
                Object.keys(contacts).map(function (key) {
                    if (!userPicture && contacts[key].category === "Email")
                        setUserPicture(contacts[key].value);
                });

                setContacts(contacts);
            }
        } catch (e) {
            console.log(e)
        }
    }

    const GetRequest = async () => {
        try {
            const request_key = await window.contract.get_request_key({
                account_id: window.accountId
            });

            if (request_key) {
                const request = await window.contract.get_request({
                    request_key: request_key
                });

                let queryHasKey = false;
                if (location.search) {
                    const query = JSON.parse(JSON.stringify(queryString.parse(location.search)));
                    queryHasKey = query && query.hasOwnProperty("key");
                }

                if (!queryHasKey && request && request.contact && request.contact.value) {
                    setHideStorageDeposit(true);
                    setComplete("Auth request found. Processing... ");
                    console.log("Request found");
                    const request = JSON.parse(window.localStorage.getItem('request'));
                    if (request.hasOwnProperty("request_key")) {
                        fetch("telegram.php", {
                            method: 'POST',
                            body: JSON.stringify({
                                operation: "send",
                                contact: request.contact,
                                contact_type: request.category || request.contact_type,
                                telegram_id: request.value,
                                request_key: request.request_key,
                                account_id: window.accountId,
                                network: config.networkId,
                                additional_contact: request.additional_contact
                            }),
                            headers: {
                                'Accept': 'application/json',
                                'Content-Type': 'application/json'
                            }
                        })
                            .then(response => response.json())
                            .then(data => {
                                if (data.status) {
                                    setComplete(data.text);
                                    setWarning("");
                                } else {
                                    setWarning(data.text);
                                    setComplete("");
                                    if (data.text == "Active request found, key already sent") {
                                        setHideStorageDeposit(true);
                                    }
                                }
                            })
                            .catch(err => console.error("Error:", err));
                    }
                }
            } else {
                window.localStorage.setItem('request', "[]");
            }
        } catch (e) {
            console.log(e)
        }
    };

    const ChangeContactType = (value) => {
        setIsInputContactFormAvailable(value !== "Github");
        setIsGithubLoginAvailable(value === "Github");

        setContactType(value)
    }

    return (
        // use React Fragment, <>, to avoid wrapping elements in unnecessary divs
        <>
            <Header/>
            <main>
                <div className="background-img"></div>
                <h1>
                    Near {appSettings.appNme}
                </h1>

                <Warning/>
                <Complete/>
                <WhiteListedKeyRemove/>


                {(showStorageDepositMenuClick || (!hideStorageDeposit && storagePaid < REQUIRED_STORAGE_DEPOSIT)) &&
                <div style={{paddingBottom: "20px"}}>
                    <fieldset id="fieldset-storage-deposit">
                        <label
                            htmlFor="storage-deposit"
                            style={{
                                display: 'block',
                                color: 'var(--gray)',
                                marginBottom: '0.5em'
                            }}
                        >
                            {storagePaid < REQUIRED_STORAGE_DEPOSIT
                                ? `Storage deposit is needed, at least ${REQUIRED_STORAGE_DEPOSIT} NEAR`
                                : "Storage Deposit"
                            }
                            :
                        </label>
                        <div style={{display: 'flex', paddingBottom: "15px"}}>

                            <input
                                autoComplete="off"
                                defaultValue={inputStorageDeposit}
                                id="storage-deposit"
                                onChange={e => setInputStorageDeposit(e.target.value)}
                                placeholder="Enter amount"
                                style={{flex: 1}}
                            />
                            <button
                                style={{borderRadius: '0 5px 5px 0'}}
                                onClick={async event => {
                                    event.preventDefault()
                                    try {
                                        await window.contract.storage_deposit({},
                                            100000000000000, ConvertToYoctoNear(inputStorageDeposit)).then(() => GetStoragePaid());

                                    } catch (e) {
                                        alert(
                                            'Something went wrong! \n' +
                                            'Check your browser console for more info.\n' +
                                            e.toString()
                                        )
                                        throw e
                                    }


                                }}
                            >
                                Deposit
                            </button>
                        </div>
                        {storagePaid > 0 &&
                        <button
                            style={{borderRadius: '5px'}}
                            onClick={async event => {
                                event.preventDefault()
                                try {
                                    await window.contract.storage_withdraw({},
                                        100000000000000, 1).then(() => GetStoragePaid());

                                } catch (e) {
                                    alert(
                                        'Something went wrong! \n' +
                                        'Check your browser console for more info.\n' +
                                        e.toString()
                                    )
                                    throw e
                                }
                            }}
                        >
                            Withdraw {storagePaid} from storage
                        </button>
                        }
                    </fieldset>
                </div>
                }

                <form className="main-form" onSubmit={async event => {
                    event.preventDefault()
                    /* генерим ключи на сервере, сразу сохраняем по account_id
                    когда есть реквест, проверяем ключ из базы, отправляем
                    когда подписываем, удаляем ключ из базы
                     */

                    // get elements from the form using their id attribute
                    const {fieldset, contact} = event.target.elements
                    fieldset.disabled = true;
                    setButtonDisabled(true);


                    const has_request_key = await window.contract.has_request_key({
                        account_id: window.accountId
                    });

                    if (has_request_key) {
                        setWarning("Previous auth attempt was not finished. Please abort previous attempt first.");
                        setWhiteListedKeyRemove(true);
                        return;
                    }

                    try {
                        if (contact && contactType) {

                            await fetch("telegram.php", {
                                method: 'POST',
                                body: JSON.stringify({
                                    operation: "start",
                                    contact: contact.value,
                                    account_id: window.accountId,
                                    contact_type: contactType,
                                    network: config.networkId
                                }),
                                headers: {
                                    'Accept': 'application/json',
                                    'Content-Type': 'application/json'
                                }
                            })
                                .then(response => response.json())
                                .then(async data => {
                                    console.log(data);
                                    if (!data.status) {
                                        setWarning(data.text);
                                        if (data.value === "already_has_key")
                                            setWhiteListedKeyRemove(true);
                                    } else {
                                        setWarning("");
                                        console.log("contact");
                                        console.log(contact);
                                        data.contact = contact.value;
                                        data.contact_type = contactType.toLowerCase()
                                        window.localStorage.setItem('request', data ? JSON.stringify(data) : "[]");

                                        let contactMetadata =
                                            (data.contact_type === "telegram")
                                                ? {
                                                    category: contactType,
                                                    value: contact.value,
                                                    account_id: Number(data.value)
                                                }
                                                : {
                                                    category: contactType,
                                                    value: contact.value
                                                };

                                        try {
                                            await window.contract.start_auth({
                                                request_key: data.request_key,
                                                contact: contactMetadata,
                                            }, FULL_GAS, 1)
                                        } catch (e) {
                                            ContractCallAlert();
                                            throw e
                                        }

                                        /*
                                        console.log("3")
                                        if (!warning)
                                            fetch("telegram.php", {
                                                method: 'POST',
                                                body: JSON.stringify({
                                                    operation: "send",
                                                    telegram_id: data.value,
                                                    key: keypair.secretKey
                                                }),
                                                headers: {
                                                    'Accept': 'application/json',
                                                    'Content-Type': 'application/json'
                                                }
                                            })
                                                .then(response => response.json())
                                                .then(data => {
                                                    if (data.status) {
                                                        setComplete(data.text);
                                                    }
                                                })
                                                .catch(err => console.error("Error:", err));
                                        */
                                    }
                                });
                        }
                    } catch (e) {
                        ContractCallAlert();
                        throw e
                    } finally {
                        // re-enable the form, whether the call succeeded or failed
                        fieldset.disabled = false;
                        setButtonDisabled(false);
                    }

                    // update local `greeting` variable to match persisted value
                    //set_greeting(newGreeting)

                    // show Notification
                    setShowNotification(true)

                    // remove Notification again after css animation completes
                    // this allows it to be shown again next time the form is submitted
                    setTimeout(() => {
                        setShowNotification(false)
                    }, 11000)
                }}>
                    <fieldset id="fieldset">
                        <label
                            htmlFor="contact"
                            style={{
                                display: 'block',
                                color: 'var(--gray)',
                                marginBottom: '0.5em'
                            }}
                        >
                            Auth social account
                        </label>
                        <div style={{display: 'flex'}}>

                            <Dropdown
                                options={dropdownOptions}
                                onChange={e => ChangeContactType(e.value)}
                                value={dropdownOptions[0]}
                                placeholder="Select an option"/>


                            {isInputContactFormAvailable &&
                            <>
                                <input
                                    autoComplete="off"
                                    defaultValue=""
                                    id="contact"
                                    onChange={e => setButtonDisabled(!e.target.value)}
                                    placeholder="Enter account handler"
                                    style={{flex: 1}}
                                />
                                <button
                                    disabled={buttonDisabled || incufficientStorage}
                                    style={{borderRadius: '0 5px 5px 0'}}
                                >
                                    Send
                                </button>
                            </>
                            }

                            <LoginGithub/>

                        </div>
                    </fieldset>
                </form>

                <Contacts/>

                {showSendForm ?
                    <div className="send-form">
                        <fieldset id="fieldset">
                            <label
                                htmlFor="contact"
                                style={{
                                    display: 'block',
                                    color: 'var(--gray)',
                                    marginBottom: '0.5em'
                                }}
                            >
                                Send tokens by social network handler
                            </label>
                            <div style={{display: 'flex'}}>

                                <Dropdown
                                    options={dropdownOptions}
                                    onChange={e => setSendFormType(e.value)}
                                    value={sendFormType}
                                    placeholder="Select an option"/>

                                <input
                                    autoComplete="off"
                                    defaultValue={sendFormContact}
                                    id="send-contact"
                                    className="send-contact"
                                    onChange={e => setSendFormContact(e.target.value)}
                                    placeholder="Enter account handler"
                                    style={{flex: 1}}
                                />

                                <input
                                    autoComplete="off"
                                    defaultValue={sendFormNearAmount}
                                    id="send-amount"
                                    className="send-amount"
                                    onChange={e => setSendFormNearAmount(e.target.value)}
                                    placeholder="Enter amount"
                                />

                                <button
                                    style={{borderRadius: '0 5px 5px 0'}}
                                    onClick={async event => {
                                        event.preventDefault()
                                        try {
                                            let owner = "";
                                            if(sendFormType.toLowerCase() !== "telegram") {
                                                owner = await window.contract.get_account_for_contact({
                                                    "contact":
                                                        {"category": sendFormType, "value": sendFormContact}
                                                });
                                            }

                                            console.log(sendFormContact + " " + owner);

                                            if (owner !== "") {
                                                setSendFormResult("Contact was not found. Abort")
                                            } else {
                                                try {
                                                    await window.contract.send({
                                                        "contact":
                                                            {"category": sendFormType, "value": sendFormContact}
                                                    }, FULL_GAS, ConvertToYoctoNear(sendFormNearAmount))

                                                    setShowNotification({method: "call", data: "send"});
                                                    setTimeout(() => {
                                                        setShowNotification("")
                                                    }, 11000)
                                                } catch (e) {
                                                    ContractCallAlert();
                                                    throw e
                                                }
                                            }


                                        } catch (e) {
                                            alert(
                                                'Something went wrong! \n' +
                                                'Check your browser console for more info.\n' +
                                                e.toString()
                                            )
                                            throw e
                                        }


                                    }}
                                >
                                    Send
                                </button>

                            </div>
                        </fieldset>

                        {sendFormResult ?
                            <div className="warning">{sendFormResult}</div>
                            : null
                        }
                    </div>
                    : null
                }


            </main>
            <Footer/>


            {showNotification && Object.keys(showNotification) &&
            <Notification method={showNotification.method} data={showNotification.data}/>}
            <ReactTooltip/>
        </>
    )
}

function getNearAccountConnection() {
    if (!window.connection) {
        const provider = new nearAPI.providers.JsonRpcProvider(config.nodeUrl);
        window.connection = new nearAPI.Connection(config.nodeUrl, provider, {});
    }
    return window.connection;
}

function Notification(props) {
    const urlPrefix = `https://explorer.${config.networkId}.near.org/accounts`
    if (props.method === "call")
        return (
            <aside>
                <a target="_blank" rel="noreferrer" href={`${urlPrefix}/${window.accountId}`}>
                    {window.accountId}
                </a>
                {' '/* React trims whitespace around tags; insert literal space character when needed */}
                called method: '{props.data}' in contract:
                {' '}
                <a target="_blank" rel="noreferrer" href={`${urlPrefix}/${window.contract.contractId}`}>
                    {window.contract.contractId}
                </a>
                <footer>
                    <div>✔ Succeeded</div>
                    <div>Just now</div>
                </footer>
            </aside>
        )
    else if (props.method === "text")
        return (
            <aside>
                {props.data}
                <footer>
                    <div>✔ Succeeded</div>
                    <div>Just now</div>
                </footer>
            </aside>
        )
    else return (
            <aside/>
        )
}

function ContractCallAlert() {
    alert(
        'Something went wrong! ' +
        'Maybe you need to sign out and back in? ' +
        'Check your browser console for more info.'
    );
}