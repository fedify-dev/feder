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
use crate::storage::SqliteStore;
use crate::webfinger::webfinger;
use crate::{actor::actor, inbox::inbox};
use axum::routing::post;
use axum::{Router, extract::DefaultBodyLimit, http::StatusCode, routing::get};
use feder_core::FederCore;
use feder_vocab::Actor;

#[derive(Clone)]
pub struct AppState {
    pub core: Arc<Mutex<FederCore>>,
    pub store: Arc<Mutex<SqliteStore>>,
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

        let core = FederCore::new();
        let store = match &config.storage {
            StorageConfig::InMemory => SqliteStore::open_in_memory()?,
            StorageConfig::Sqlite { path } => SqliteStore::open(path)?,
        };

        Ok(Self {
            core: Arc::new(Mutex::new(core)),
            store: Arc::new(Mutex::new(store)),
            local_actor: actor,
            username: config.username,
            handle_host: config.handle_host,
            inbox_auth_policy: config.inbox_auth_policy,
        })
    }
}

pub fn build_router(config: RuntimeConfig) -> Result<Router, Error> {
    let state = AppState::from_config(config)?;

    Ok(Router::new()
        .route("/healthz", get(healthz))
        .route("/.well-known/webfinger", get(webfinger))
        .route("/users/{username}", get(actor))
        .route("/users/{username}/inbox", post(inbox))
        .layer(DefaultBodyLimit::max(1_048_576))
        .with_state(state))
}

async fn healthz() -> StatusCode {
    StatusCode::NO_CONTENT
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    use crate::{build_router, config::test_config};

    #[tokio::test]
    async fn returns_health_check() {
        let app = build_router(test_config()).expect("build router");

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .expect("valid request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }
}
