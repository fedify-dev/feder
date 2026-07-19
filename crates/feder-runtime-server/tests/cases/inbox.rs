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
    Router,
    body::Body,
    http::{Request, StatusCode, header::CONTENT_TYPE},
};
use feder_runtime_server::{
    app::router_with_state,
    config::{InboxAuthPolicy, StorageConfig},
    storage::RuntimeStore,
};
use serde_json::json;
use tower::ServiceExt;

use crate::common::{temporary_database_path, test_app_state, test_config, test_router};

fn follow_body() -> Vec<u8> {
    serde_json::to_vec(&json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Follow",
        "id": "https://remote.example/activities/follow-1",
        "actor": {
            "@context": "https://www.w3.org/ns/activitystreams",
            "type": "Person",
            "id": "https://remote.example/users/bob",
            "inbox": "https://remote.example/users/bob/inbox",
            "outbox": "https://remote.example/users/bob/outbox"
        },
        "object": "http://127.0.0.1:3000/users/alice"
    }))
    .expect("serialize follow")
}

async fn post_inbox(
    app: Router,
    uri: &str,
    content_type: &str,
    body: impl Into<Body>,
) -> axum::response::Response {
    app.oneshot(
        Request::builder()
            .method("POST")
            .uri(uri)
            .header(CONTENT_TYPE, content_type)
            .body(body.into())
            .expect("valid request"),
    )
    .await
    .expect("response")
}

#[tokio::test]
async fn valid_follow_reaches_core() {
    let state = test_app_state(test_config()).expect("build app state");
    let response = post_inbox(
        router_with_state(state.clone()),
        "/users/alice/inbox",
        "application/activity+json",
        follow_body(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let core = state.core.lock().expect("core lock");
    assert_eq!(core.state().followers().len(), 1);
    assert_eq!(
        core.state().followers()[0].follower.as_str(),
        "https://remote.example/users/bob"
    );
    assert_eq!(
        core.state().followers()[0].following.as_str(),
        "http://127.0.0.1:3000/users/alice"
    );
    assert_eq!(core.state().delivery_targets().len(), 1);
    assert_eq!(
        core.state().delivery_targets()[0].inbox.as_str(),
        "https://remote.example/users/bob/inbox"
    );
}

#[tokio::test]
async fn require_signed_rejects_unsigned_follow_before_core() {
    let mut config = test_config();
    config.inbox_auth_policy = InboxAuthPolicy::RequireSigned;
    let state = test_app_state(config).expect("build app state");
    let response = post_inbox(
        router_with_state(state.clone()),
        "/users/alice/inbox",
        "application/activity+json",
        follow_body(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(
        state
            .core
            .lock()
            .expect("core lock")
            .state()
            .followers()
            .is_empty()
    );
}

#[tokio::test]
async fn rejects_unknown_inbox_actor() {
    let state = test_app_state(test_config()).expect("build app state");
    let response = post_inbox(
        router_with_state(state.clone()),
        "/users/bob/inbox",
        "application/activity+json",
        follow_body(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(
        state
            .core
            .lock()
            .expect("core lock")
            .state()
            .followers()
            .is_empty()
    );
}

#[tokio::test]
async fn rejects_unsupported_content_type() {
    let state = test_app_state(test_config()).expect("build app state");
    let response = post_inbox(
        router_with_state(state.clone()),
        "/users/alice/inbox",
        "application/json",
        follow_body(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert!(
        state
            .core
            .lock()
            .expect("core lock")
            .state()
            .followers()
            .is_empty()
    );
}

#[tokio::test]
async fn rejects_malformed_json() {
    let state = test_app_state(test_config()).expect("build app state");
    let response = post_inbox(
        router_with_state(state.clone()),
        "/users/alice/inbox",
        "application/activity+json",
        "{not json",
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(
        state
            .core
            .lock()
            .expect("core lock")
            .state()
            .followers()
            .is_empty()
    );
}

#[tokio::test]
async fn ignores_unsupported_activity_without_mutating_core() {
    let state = test_app_state(test_config()).expect("build app state");
    let body = serde_json::to_vec(&json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Create",
        "id": "https://remote.example/activities/create-1",
        "actor": "https://remote.example/users/bob",
        "object": {
            "type": "Note",
            "id": "https://remote.example/notes/1"
        }
    }))
    .expect("serialize create");
    let response = post_inbox(
        router_with_state(state.clone()),
        "/users/alice/inbox",
        "application/activity+json",
        body,
    )
    .await;

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert!(
        state
            .core
            .lock()
            .expect("core lock")
            .state()
            .followers()
            .is_empty()
    );
}

#[tokio::test]
async fn rejects_oversized_inbox_body() {
    let response = post_inbox(
        test_router(test_config()).expect("build router"),
        "/users/alice/inbox",
        "application/activity+json",
        vec![b' '; 1_048_577],
    )
    .await;

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn sqlite_storage_persists_followers_across_app_state_reopen() {
    let path = temporary_database_path("feder-runtime-server-test");
    let mut config = test_config();
    config.storage = StorageConfig::Sqlite { path: path.clone() };
    let state = test_app_state(config).expect("build app state");
    let response = post_inbox(
        router_with_state(state.clone()),
        "/users/alice/inbox",
        "application/activity+json",
        follow_body(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    drop(state);

    let mut config = test_config();
    config.storage = StorageConfig::Sqlite { path: path.clone() };
    let state = test_app_state(config).expect("reopen app state");
    let followers = state
        .store
        .lock()
        .expect("store lock")
        .list_followers(
            &"http://127.0.0.1:3000/users/alice"
                .parse()
                .expect("valid IRI"),
        )
        .expect("list followers");

    assert_eq!(followers.len(), 1);
    assert_eq!(
        followers[0].follower.as_str(),
        "https://remote.example/users/bob"
    );

    drop(state);
    let _ = std::fs::remove_file(path);
}
