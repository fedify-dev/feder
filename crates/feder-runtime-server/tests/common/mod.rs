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

use axum::{
    Router,
    body::Bytes,
    http::{HeaderMap, StatusCode, Uri},
    routing::post,
};
use feder_core::{FederConfig, FederCore, http_signatures::ActorKeyPair};
use feder_runtime_server::{
    Error,
    app::{AppState, router_with_state},
    config::{InboxAuthPolicy, OutboundAddressPolicy, RuntimeConfig, StorageConfig},
    send::ActivitySender,
    storage::{RuntimeStore, SqliteStore},
};
use feder_vocab::{Actor, CryptographicKey, Reference};
use iri_string::types::IriFragmentStr;
use tokio::{sync::mpsc, task::JoinHandle};

pub struct RecordedRequest {
    pub headers: HeaderMap,
    pub uri: Uri,
    pub body: Bytes,
}

pub async fn spawn_inbox_server(
    response_status: StatusCode,
) -> (String, mpsc::Receiver<RecordedRequest>, JoinHandle<()>) {
    let (sender, receiver) = mpsc::channel(1);
    let app = Router::new().route(
        "/inbox",
        post(move |headers: HeaderMap, uri: Uri, body: Bytes| {
            let sender = sender.clone();
            async move {
                sender
                    .send(RecordedRequest { headers, uri, body })
                    .await
                    .expect("request receiver remains open");
                response_status
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind inbox server");
    let address = listener.local_addr().expect("inbox server address");
    let task = tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("serve inbox endpoint");
    });

    (format!("http://{address}/inbox"), receiver, task)
}

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
        outbound_address_policy: OutboundAddressPolicy::AllowPrivateAddress,
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
        key_id.clone(),
        actor.id.clone(),
        actor_key_pair.public_key_pem().to_string(),
    )));
    let core = FederCore::new(FederConfig::new(actor.clone()));
    let actor_key_pair = Arc::new(actor_key_pair);
    let activity_sender = ActivitySender::new(
        actor_key_pair.clone(),
        key_id.to_string(),
        config.outbound_address_policy,
    )?;

    Ok(AppState {
        core: Arc::new(Mutex::new(core)),
        store: Arc::new(Mutex::new(store)),
        actor_key_pair,
        activity_sender,
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

pub fn test_activity_sender() -> ActivitySender {
    test_activity_sender_with_policy(OutboundAddressPolicy::AllowPrivateAddress)
}

pub fn test_activity_sender_with_policy(policy: OutboundAddressPolicy) -> ActivitySender {
    ActivitySender::new(
        Arc::new(fixture_actor_key_pair().expect("load actor key pair fixture")),
        "https://local.example/users/alice#main-key".to_string(),
        policy,
    )
    .expect("build activity sender")
}
