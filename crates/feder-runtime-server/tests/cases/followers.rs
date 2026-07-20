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
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use feder_core::{Action, RemoveFollower, StoreFollower};
use feder_runtime_server::{app::router_with_state, storage::RuntimeStore};
use feder_vocab::{Iri, Reference};
use serde_json::Value;
use tower::ServiceExt;

use crate::common::{test_app_state, test_config, test_router};

fn iri(value: &str) -> Iri {
    value.parse().expect("valid test IRI")
}

async fn get_followers(app: axum::Router, username: &str) -> axum::response::Response {
    app.oneshot(
        Request::builder()
            .uri(format!("/users/{username}/followers"))
            .header(header::ACCEPT, "application/activity+json")
            .body(Body::empty())
            .expect("valid request"),
    )
    .await
    .expect("response")
}

async fn response_json(response: axum::response::Response) -> Value {
    let body = to_bytes(response.into_body(), 4096)
        .await
        .expect("read response body");
    serde_json::from_slice(&body).expect("valid JSON")
}

fn store_follower(follower: &str) -> Action {
    Action::StoreFollower(StoreFollower {
        follower: Reference::id(iri(follower)),
        following: Reference::id(iri("http://127.0.0.1:3000/users/alice")),
    })
}

#[tokio::test]
async fn returns_empty_followers_collection_with_activitypub_headers() {
    let response = get_followers(test_router(test_config()).expect("build router"), "alice").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/activity+json"
    );
    assert_eq!(response.headers().get(header::VARY).unwrap(), "Accept");

    let json = response_json(response).await;
    assert_eq!(json["@context"], "https://www.w3.org/ns/activitystreams");
    assert_eq!(json["type"], "OrderedCollection");
    assert_eq!(json["id"], "http://127.0.0.1:3000/users/alice/followers");
    assert_eq!(json["totalItems"], 0);
    assert_eq!(json["orderedItems"], serde_json::json!([]));
}

#[tokio::test]
async fn returns_stored_followers_and_count() {
    let state = test_app_state(test_config()).expect("build app state");
    state
        .store
        .lock()
        .expect("store lock")
        .persist_actions(&[
            store_follower("https://remote.example/users/carol"),
            store_follower("https://remote.example/users/bob"),
        ])
        .expect("persist followers");

    let response = get_followers(router_with_state(state), "alice").await;
    assert_eq!(response.status(), StatusCode::OK);

    let json = response_json(response).await;
    assert_eq!(json["totalItems"], 2);
    assert_eq!(
        json["orderedItems"],
        serde_json::json!([
            "https://remote.example/users/bob",
            "https://remote.example/users/carol"
        ])
    );
}

#[tokio::test]
async fn reflects_follower_removal() {
    let state = test_app_state(test_config()).expect("build app state");
    {
        let mut store = state.store.lock().expect("store lock");
        store
            .persist_actions(&[store_follower("https://remote.example/users/bob")])
            .expect("persist follower");
        store
            .persist_actions(&[Action::RemoveFollower(RemoveFollower {
                follower: iri("https://remote.example/users/bob"),
                following: state.local_actor.id.clone(),
            })])
            .expect("remove follower");
    }

    let response = get_followers(router_with_state(state), "alice").await;
    assert_eq!(response.status(), StatusCode::OK);

    let json = response_json(response).await;
    assert_eq!(json["totalItems"], 0);
    assert_eq!(json["orderedItems"], serde_json::json!([]));
}

#[tokio::test]
async fn rejects_unknown_username() {
    let response =
        get_followers(test_router(test_config()).expect("build router"), "unknown").await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn rejects_followers_request_when_html_is_preferred() {
    let response = test_router(test_config())
        .expect("build router")
        .oneshot(
            Request::builder()
                .uri("/users/alice/followers")
                .header(header::ACCEPT, "text/html, application/activity+json;q=0.8")
                .body(Body::empty())
                .expect("valid request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::NOT_ACCEPTABLE);
    assert_eq!(response.headers().get(header::VARY).unwrap(), "Accept");
}

#[tokio::test]
async fn rejects_followers_request_without_activitypub_accept() {
    let response = test_router(test_config())
        .expect("build router")
        .oneshot(
            Request::builder()
                .uri("/users/alice/followers")
                .body(Body::empty())
                .expect("valid request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::NOT_ACCEPTABLE);
    assert_eq!(response.headers().get(header::VARY).unwrap(), "Accept");
}
