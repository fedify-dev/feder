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

use crate::config::RuntimeConfig;
use crate::webfinger::webfinger;
use axum::{Router, http::StatusCode, routing::get};
use feder_core::{FederConfig, FederCore};
use feder_vocab::{Actor, Iri};

#[derive(Clone)]
pub struct AppState {
    pub core: Arc<Mutex<FederCore>>,
    pub actor_id: Iri,
    pub username: String,
    pub handle_host: String,
}

impl AppState {
    pub fn from_config(config: &RuntimeConfig) -> Self {
        let actor = Actor::person(
            config.actor_id.clone(),
            config.inbox.clone(),
            config.outbox.clone(),
        );

        let core = FederCore::new(FederConfig::new(actor));

        Self {
            core: Arc::new(Mutex::new(core)),
            actor_id: config.actor_id.clone(),
            username: config.username.clone(),
            handle_host: config.handle_host.clone(),
        }
    }
}

pub fn build_router(config: &RuntimeConfig) -> Router {
    let state = AppState::from_config(config);

    Router::new()
        .route("/healthz", get(healthz))
        .route("/.well-known/webfinger", get(webfinger))
        .with_state(state)
}

async fn healthz() -> StatusCode {
    StatusCode::NO_CONTENT
}
