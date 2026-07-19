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

use crate::Error;
use crate::config::{InboxAuthPolicy, RuntimeConfig, StorageConfig};
use crate::send::ActivitySender;
use crate::storage::{RuntimeStore, SqliteStore};
use crate::webfinger::webfinger;
use crate::{actor::actor, inbox::inbox};
use axum::routing::post;
use axum::{Router, extract::DefaultBodyLimit, http::StatusCode, routing::get};
use feder_core::{
    FederConfig, FederCore,
    http_signatures::{ActorKeyPair, generate_actor_key_pair},
};
use feder_vocab::{Actor, CryptographicKey, Reference};
use iri_string::types::IriFragmentStr;
use rand_core::OsRng;

#[derive(Clone)]
pub struct AppState {
    pub core: Arc<Mutex<FederCore>>,
    pub store: Arc<Mutex<SqliteStore>>,
    pub actor_key_pair: Arc<ActorKeyPair>,
    pub activity_sender: ActivitySender,
    pub local_actor: Actor,
    pub username: String,
    pub handle_host: String,
    pub inbox_auth_policy: InboxAuthPolicy,
}

impl AppState {
    pub fn from_config(config: RuntimeConfig) -> Result<Self, Error> {
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
                let key_pair = generate_actor_key_pair(&mut OsRng)?;
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
        let activity_sender = ActivitySender::new()?;

        Ok(Self {
            core: Arc::new(Mutex::new(core)),
            store: Arc::new(Mutex::new(store)),
            actor_key_pair: Arc::new(actor_key_pair),
            activity_sender,
            local_actor: actor,
            username: config.username,
            handle_host: config.handle_host,
            inbox_auth_policy: config.inbox_auth_policy,
        })
    }
}

pub fn build_router(config: RuntimeConfig) -> Result<Router, Error> {
    let state = AppState::from_config(config)?;

    Ok(router_with_state(state))
}

pub fn router_with_state(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/.well-known/webfinger", get(webfinger))
        .route("/users/{username}", get(actor))
        .route("/users/{username}/inbox", post(inbox))
        .layer(DefaultBodyLimit::max(1_048_576))
        .with_state(state)
}

async fn healthz() -> StatusCode {
    StatusCode::NO_CONTENT
}
