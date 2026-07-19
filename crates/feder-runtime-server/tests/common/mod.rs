// Feder: A portable ActivityPub core for many runtimes.
// Copyright (C) 2026 Feder contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, version 3.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use std::sync::{Arc, Mutex};

use axum::Router;
use feder_core::{FederConfig, FederCore, http_signatures::ActorKeyPair};
use feder_runtime_server::{
    Error,
    app::{AppState, router_with_state},
    config::{InboxAuthPolicy, RuntimeConfig, StorageConfig},
    storage::{RuntimeStore, SqliteStore},
};
use feder_vocab::{Actor, CryptographicKey, Reference};
use iri_string::types::IriFragmentStr;

pub fn test_config() -> RuntimeConfig {
    RuntimeConfig {
        actor_id: "http://127.0.0.1:3000/users/alice"
            .parse()
            .expect("valid actor IRI"),
        inbox: "http://127.0.0.1:3000/users/alice/inbox"
            .parse()
            .expect("valid inbox IRI"),
        outbox: "http://127.0.0.1:3000/users/alice/outbox"
            .parse()
            .expect("valid outbox IRI"),
        bind: "127.0.0.1:3000".parse().expect("valid bind address"),
        username: "alice".to_string(),
        handle_host: "127.0.0.1:3000".to_string(),
        inbox_auth_policy: InboxAuthPolicy::AllowUnsignedInsecureDev,
        storage: StorageConfig::InMemory,
    }
}

pub fn test_app_state(config: RuntimeConfig) -> Result<AppState, Error> {
    let mut actor = Actor::person(config.actor_id, config.inbox, config.outbox);
    actor.preferred_username = Some(config.username.clone());
    actor.name = Some(config.username.clone());

    let mut store = match &config.storage {
        StorageConfig::InMemory => SqliteStore::open_in_memory()?,
        StorageConfig::Sqlite { path } => SqliteStore::open(path)?,
    };
    let actor_key_pair = match store.load_actor_key_pair(&actor.id)? {
        Some(key_pair) => key_pair,
        None => {
            let key_pair = fixture_actor_key_pair()?;
            store.insert_actor_key_pair(&actor.id, &key_pair)?;
            key_pair
        }
    };
    let mut key_id = actor.id.clone();
    key_id.set_fragment(Some(
        IriFragmentStr::new("main-key").expect("main-key is a valid IRI fragment"),
    ));
    actor.set_public_key(Reference::object(CryptographicKey::new(
        key_id,
        actor.id.clone(),
        actor_key_pair.public_key_pem().to_string(),
    )));
    let core = FederCore::new(FederConfig::new(actor.clone()));

    Ok(AppState {
        core: Arc::new(Mutex::new(core)),
        store: Arc::new(Mutex::new(store)),
        actor_key_pair: Arc::new(actor_key_pair),
        local_actor: actor,
        username: config.username,
        handle_host: config.handle_host,
        inbox_auth_policy: config.inbox_auth_policy,
    })
}

pub fn test_router(config: RuntimeConfig) -> Result<Router, Error> {
    Ok(router_with_state(test_app_state(config)?))
}

pub fn temporary_database_path(prefix: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "{prefix}-{}-{}.sqlite3",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time after unix epoch")
            .as_nanos()
    ))
}

fn fixture_actor_key_pair() -> Result<ActorKeyPair, feder_core::http_signatures::KeyError> {
    ActorKeyPair::from_pem(
        include_str!("../fixtures/rsa-private-key.pem").to_string(),
        include_str!("../fixtures/rsa-public-key.pem").to_string(),
    )
}
