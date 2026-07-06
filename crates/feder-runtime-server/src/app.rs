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

use crate::actor::actor;
use crate::config::RuntimeConfig;
use crate::webfinger::webfinger;
use axum::{Router, http::StatusCode, routing::get};
use feder_core::{FederConfig, FederCore};
use feder_vocab::Actor;

#[derive(Clone)]
pub struct AppState {
    pub core: Arc<Mutex<FederCore>>,
    pub local_actor: Actor,
    pub username: String,
    pub handle_host: String,
}

impl AppState {
    pub fn from_config(config: RuntimeConfig) -> Self {
        let mut actor = Actor::person(config.actor_id, config.inbox, config.outbox);
        actor.preferred_username = Some(config.username.clone());
        actor.name = Some(config.username.clone());

        let core = FederCore::new(FederConfig::new(actor.clone()));

        Self {
            core: Arc::new(Mutex::new(core)),
            local_actor: actor,
            username: config.username,
            handle_host: config.handle_host,
        }
    }
}

pub fn build_router(config: RuntimeConfig) -> Router {
    let state = AppState::from_config(config);

    Router::new()
        .route("/healthz", get(healthz))
        .route("/.well-known/webfinger", get(webfinger))
        .route("/users/{username}", get(actor))
        .route("/users/{identifier}/inbox", post(inbox))
        .with_state(state)
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
        let app = build_router(test_config());

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
