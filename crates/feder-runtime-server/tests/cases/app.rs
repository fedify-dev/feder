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

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use feder_runtime_server::{AppState, config::StorageConfig, storage::RuntimeStore};
use feder_vocab::Reference;
use tower::ServiceExt;

use crate::common::{temporary_database_path, test_config, test_router};

#[tokio::test]
async fn returns_health_check() {
    let app = test_router(test_config()).expect("build router");

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

#[test]
fn startup_generates_then_reuses_persisted_actor_key_pair() {
    let path = temporary_database_path("feder-startup-key-test");
    let mut config = test_config();
    config.storage = StorageConfig::Sqlite { path: path.clone() };
    let first = AppState::from_config(config).expect("build first app state");
    let expected_public_key = first.actor_key_pair.public_key_pem().to_string();
    let Reference::Object(published_key) = first
        .local_actor
        .public_key
        .as_ref()
        .expect("actor publishes public key")
    else {
        panic!("actor public key should be embedded");
    };
    assert_eq!(
        published_key.id.as_str(),
        "http://127.0.0.1:3000/users/alice#main-key"
    );
    assert_eq!(published_key.owner, first.local_actor.id);
    assert_eq!(published_key.public_key_pem, expected_public_key);
    let stored = first
        .store
        .lock()
        .expect("lock store")
        .load_actor_key_pair(&first.local_actor.id)
        .expect("load actor key pair")
        .expect("stored actor key pair");
    assert_eq!(stored, *first.actor_key_pair);
    drop(first);

    let mut config = test_config();
    config.storage = StorageConfig::Sqlite { path: path.clone() };
    let second = AppState::from_config(config).expect("reopen app state");

    assert_eq!(second.actor_key_pair.public_key_pem(), expected_public_key);

    drop(second);
    let _ = std::fs::remove_file(path);
}
