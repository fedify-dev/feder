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
use serde_json::Value;
use tower::ServiceExt;

use crate::common::{test_config, test_router};

#[tokio::test]
async fn returns_local_actor() {
    let app = test_router(test_config()).expect("build router");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/users/alice")
                .header(header::ACCEPT, "application/activity+json")
                .body(Body::empty())
                .expect("valid request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/activity+json"
    );
    assert_eq!(response.headers().get(header::VARY).unwrap(), "Accept");

    let body = to_bytes(response.into_body(), 2048)
        .await
        .expect("read response body");
    let json: Value = serde_json::from_slice(&body).expect("valid json");

    assert_eq!(
        json["@context"],
        serde_json::json!([
            "https://www.w3.org/ns/activitystreams",
            "https://w3id.org/security/v1"
        ])
    );
    assert_eq!(json["type"], "Person");
    assert_eq!(json["id"], "http://127.0.0.1:3000/users/alice");
    assert_eq!(json["inbox"], "http://127.0.0.1:3000/users/alice/inbox");
    assert_eq!(json["outbox"], "http://127.0.0.1:3000/users/alice/outbox");
    assert_eq!(
        json["followers"],
        "http://127.0.0.1:3000/users/alice/followers"
    );
    assert_eq!(json["preferredUsername"], "alice");
    assert_eq!(json["name"], "alice");
    assert_eq!(
        json["publicKey"],
        serde_json::json!({
            "id": "http://127.0.0.1:3000/users/alice#main-key",
            "type": "CryptographicKey",
            "owner": "http://127.0.0.1:3000/users/alice",
            "publicKeyPem": include_str!("../fixtures/rsa-public-key.pem"),
        })
    );
}

#[tokio::test]
async fn rejects_actor_request_when_html_is_preferred() {
    let response = test_router(test_config())
        .expect("build router")
        .oneshot(
            Request::builder()
                .uri("/users/alice")
                .header(header::ACCEPT, "text/html, application/activity+json;q=0.8")
                .body(Body::empty())
                .expect("valid request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::NOT_ACCEPTABLE);
}

#[tokio::test]
async fn rejects_actor_request_without_activitypub_accept() {
    let response = test_router(test_config())
        .expect("build router")
        .oneshot(
            Request::builder()
                .uri("/users/alice")
                .body(Body::empty())
                .expect("valid request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::NOT_ACCEPTABLE);
}

#[tokio::test]
async fn rejects_unknown_actor() {
    let app = test_router(test_config()).expect("build router");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/users/bob")
                .body(Body::empty())
                .expect("valid request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
