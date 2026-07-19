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
    Json, Router,
    body::{Body, Bytes},
    http::{HeaderMap, Request, StatusCode, Uri, header::CONTENT_TYPE},
    routing::{get, post},
};
use feder_core::http_signatures::{create_sha256_digest_header, sign_draft_cavage};
use feder_runtime_server::{
    app::router_with_state,
    config::{InboxAuthPolicy, StorageConfig},
    storage::RuntimeStore,
};
use serde_json::json;
use tower::ServiceExt;

use crate::common::{
    RecordedRequest, fixture_actor_key_pair, spawn_inbox_server, temporary_database_path,
    test_app_state, test_config, test_router,
};

fn follow_body() -> Vec<u8> {
    follow_body_for_inbox("https://remote.example/users/bob/inbox")
}

fn follow_body_for_inbox(inbox: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Follow",
        "id": "https://remote.example/activities/follow-1",
        "actor": {
            "@context": "https://www.w3.org/ns/activitystreams",
            "type": "Person",
            "id": "https://remote.example/users/bob",
            "inbox": inbox,
            "outbox": "https://remote.example/users/bob/outbox"
        },
        "object": "http://127.0.0.1:3000/users/alice"
    }))
    .expect("serialize follow")
}

fn id_only_follow_body(actor_id: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Follow",
        "id": format!("{actor_id}/follows/1"),
        "actor": actor_id,
        "object": "http://127.0.0.1:3000/users/alice"
    }))
    .expect("serialize ID-only follow")
}

async fn spawn_actor_server() -> (
    String,
    tokio::sync::mpsc::Receiver<RecordedRequest>,
    tokio::task::JoinHandle<()>,
) {
    let (actor_id, _key_id, receiver, task) = spawn_actor_server_inner(false).await;
    (actor_id, receiver, task)
}

async fn spawn_actor_server_with_separate_key() -> (
    String,
    String,
    tokio::sync::mpsc::Receiver<RecordedRequest>,
    tokio::task::JoinHandle<()>,
) {
    spawn_actor_server_inner(true).await
}

async fn spawn_actor_server_inner(
    separate_key: bool,
) -> (
    String,
    String,
    tokio::sync::mpsc::Receiver<RecordedRequest>,
    tokio::task::JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind actor server");
    let address = listener.local_addr().expect("actor server address");
    let actor_id = format!("http://{address}/users/bob");
    let key_id = if separate_key {
        format!("http://{address}/keys/1")
    } else {
        format!("{actor_id}#main-key")
    };
    let inbox = format!("http://{address}/inbox");
    let public_key = json!({
        "id": key_id,
        "type": "CryptographicKey",
        "owner": actor_id,
        "publicKeyPem": fixture_actor_key_pair()
            .expect("load actor key fixture")
            .public_key_pem()
    });
    let actor = json!({
        "@context": [
            "https://www.w3.org/ns/activitystreams",
            { "toot": "http://joinmastodon.org/ns#" }
        ],
        "type": "Person",
        "id": actor_id,
        "inbox": inbox,
        "outbox": format!("http://{address}/users/bob/outbox"),
        "preferredUsername": "bob",
        "endpoints": { "sharedInbox": inbox },
        "publicKey": if separate_key { json!(key_id) } else { public_key.clone() }
    });
    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    let app = Router::new()
        .route(
            "/users/bob",
            get(move || {
                let actor = actor.clone();
                async move { ([(CONTENT_TYPE, "application/activity+json")], Json(actor)) }
            }),
        )
        .route(
            "/keys/1",
            get(move || {
                let public_key = public_key.clone();
                async move {
                    (
                        [(CONTENT_TYPE, "application/activity+json")],
                        Json(public_key),
                    )
                }
            }),
        )
        .route(
            "/inbox",
            post(move |headers: HeaderMap, uri: Uri, body: Bytes| {
                let sender = sender.clone();
                async move {
                    sender
                        .send(RecordedRequest { headers, uri, body })
                        .await
                        .expect("request receiver remains open");
                    StatusCode::ACCEPTED
                }
            }),
        );
    let task = tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("serve actor endpoint");
    });

    (actor_id, key_id, receiver, task)
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

async fn post_signed_inbox(
    app: Router,
    uri: &str,
    key_id: &str,
    signed_body: &[u8],
    delivered_body: impl Into<Body>,
) -> axum::response::Response {
    let date = httpdate::fmt_http_date(std::time::SystemTime::now());
    let digest = create_sha256_digest_header(signed_body);
    let host = "local.example";
    let headers = [
        ("content-type", "application/activity+json"),
        ("date", date.as_str()),
        ("digest", digest.as_str()),
        ("host", host),
    ];
    let signature = sign_draft_cavage(
        &fixture_actor_key_pair().expect("load actor key fixture"),
        key_id,
        "POST",
        uri,
        &headers,
    )
    .expect("sign inbox request");

    app.oneshot(
        Request::builder()
            .method("POST")
            .uri(uri)
            .header(CONTENT_TYPE, "application/activity+json")
            .header("date", date)
            .header("digest", digest)
            .header("host", host)
            .header("signature", signature)
            .body(delivered_body.into())
            .expect("valid request"),
    )
    .await
    .expect("response")
}

#[tokio::test]
async fn valid_follow_reaches_core() {
    let (inbox, mut requests, inbox_server) = spawn_inbox_server(StatusCode::ACCEPTED).await;
    let state = test_app_state(test_config()).expect("build app state");
    let response = post_inbox(
        router_with_state(state.clone()),
        "/users/alice/inbox",
        "application/activity+json",
        follow_body_for_inbox(&inbox),
    )
    .await;

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    {
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
        assert_eq!(core.state().delivery_targets()[0].inbox.as_str(), inbox);
    }

    let request = requests.recv().await.expect("receive Accept request");
    assert_eq!(
        request.headers.get(CONTENT_TYPE).unwrap(),
        "application/activity+json"
    );
    let activity: serde_json::Value =
        serde_json::from_slice(&request.body).expect("valid sent activity");
    assert_eq!(activity["type"], "Accept");
    assert_eq!(activity["actor"], "http://127.0.0.1:3000/users/alice");
    assert_eq!(
        activity["object"]["id"],
        "https://remote.example/activities/follow-1"
    );
    inbox_server.abort();
}

#[tokio::test]
async fn resolves_id_only_follower_and_sends_accept() {
    let (actor_id, mut requests, actor_server) = spawn_actor_server().await;
    let state = test_app_state(test_config()).expect("build app state");
    let response = post_inbox(
        router_with_state(state.clone()),
        "/users/alice/inbox",
        "application/activity+json",
        id_only_follow_body(&actor_id),
    )
    .await;

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let followers = state
        .store
        .lock()
        .expect("store lock")
        .list_followers(&state.local_actor.id)
        .expect("list followers");
    assert_eq!(followers.len(), 1);
    assert_eq!(followers[0].follower.as_str(), actor_id);
    assert_eq!(
        followers[0]
            .inbox
            .as_ref()
            .expect("resolved inbox")
            .as_str(),
        format!("{}/inbox", actor_id.trim_end_matches("/users/bob"))
    );

    let request = requests.recv().await.expect("receive Accept request");
    assert!(request.headers.contains_key("signature"));
    let activity: serde_json::Value =
        serde_json::from_slice(&request.body).expect("valid sent activity");
    assert_eq!(activity["type"], "Accept");
    assert_eq!(activity["object"]["actor"]["id"], actor_id);
    actor_server.abort();
}

#[tokio::test]
async fn verifies_signed_id_only_follow() {
    let (actor_id, mut requests, actor_server) = spawn_actor_server().await;
    let mut config = test_config();
    config.inbox_auth_policy = InboxAuthPolicy::RequireSigned;
    let state = test_app_state(config).expect("build app state");
    let body = id_only_follow_body(&actor_id);
    let response = post_signed_inbox(
        router_with_state(state.clone()),
        "/users/alice/inbox",
        &format!("{actor_id}#main-key"),
        &body,
        body.clone(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(
        state
            .core
            .lock()
            .expect("core lock")
            .state()
            .followers()
            .len(),
        1
    );
    requests.recv().await.expect("receive Accept request");
    actor_server.abort();
}

#[tokio::test]
async fn verifies_signed_follow_with_independent_key_id() {
    let (actor_id, key_id, mut requests, actor_server) =
        spawn_actor_server_with_separate_key().await;
    let mut config = test_config();
    config.inbox_auth_policy = InboxAuthPolicy::RequireSigned;
    let state = test_app_state(config).expect("build app state");
    let body = id_only_follow_body(&actor_id);
    let response = post_signed_inbox(
        router_with_state(state.clone()),
        "/users/alice/inbox",
        &key_id,
        &body,
        body.clone(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(
        state
            .core
            .lock()
            .expect("core lock")
            .state()
            .followers()
            .len(),
        1
    );
    requests.recv().await.expect("receive Accept request");
    actor_server.abort();
}

#[tokio::test]
async fn signed_follow_rejects_tampered_body() {
    let (actor_id, _requests, actor_server) = spawn_actor_server().await;
    let mut config = test_config();
    config.inbox_auth_policy = InboxAuthPolicy::RequireSigned;
    let state = test_app_state(config).expect("build app state");
    let signed_body = id_only_follow_body(&actor_id);
    let delivered_body = id_only_follow_body("https://attacker.example/users/mallory");
    let response = post_signed_inbox(
        router_with_state(state.clone()),
        "/users/alice/inbox",
        &format!("{actor_id}#main-key"),
        &signed_body,
        delivered_body,
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
    actor_server.abort();
}

#[tokio::test]
async fn signed_follow_rejects_actor_different_from_key_owner() {
    let (actor_id, _requests, actor_server) = spawn_actor_server().await;
    let mut config = test_config();
    config.inbox_auth_policy = InboxAuthPolicy::RequireSigned;
    let state = test_app_state(config).expect("build app state");
    let body = id_only_follow_body("https://attacker.example/users/mallory");
    let response = post_signed_inbox(
        router_with_state(state.clone()),
        "/users/alice/inbox",
        &format!("{actor_id}#main-key"),
        &body,
        body.clone(),
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
    actor_server.abort();
}

#[tokio::test]
async fn send_failure_returns_bad_gateway_after_core_handling() {
    let (inbox, mut requests, inbox_server) =
        spawn_inbox_server(StatusCode::INTERNAL_SERVER_ERROR).await;
    let state = test_app_state(test_config()).expect("build app state");
    let response = post_inbox(
        router_with_state(state.clone()),
        "/users/alice/inbox",
        "application/activity+json",
        follow_body_for_inbox(&inbox),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(
        state
            .core
            .lock()
            .expect("core lock")
            .state()
            .followers()
            .len(),
        1
    );
    requests.recv().await.expect("receive failed request");
    inbox_server.abort();
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
    let (inbox, mut requests, inbox_server) = spawn_inbox_server(StatusCode::ACCEPTED).await;
    let path = temporary_database_path("feder-runtime-server-test");
    let mut config = test_config();
    config.storage = StorageConfig::Sqlite { path: path.clone() };
    let state = test_app_state(config).expect("build app state");
    let response = post_inbox(
        router_with_state(state.clone()),
        "/users/alice/inbox",
        "application/activity+json",
        follow_body_for_inbox(&inbox),
    )
    .await;

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    requests.recv().await.expect("receive Accept request");
    inbox_server.abort();
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
