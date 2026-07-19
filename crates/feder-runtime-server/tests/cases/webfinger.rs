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

const WEBFINGER_PATH: &str = "/.well-known/webfinger?resource=acct:alice@127.0.0.1:3000";

#[tokio::test]
async fn returns_webfinger_descriptor_for_local_actor() {
    let app = test_router(test_config()).expect("build router");

    let response = app
        .oneshot(
            Request::builder()
                .uri(WEBFINGER_PATH)
                .body(Body::empty())
                .expect("valid request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/jrd+json"
    );

    let body = to_bytes(response.into_body(), 1024)
        .await
        .expect("read response body");
    let json: Value = serde_json::from_slice(&body).expect("valid json");

    assert_eq!(json["subject"], "acct:alice@127.0.0.1:3000");
    assert_eq!(json["aliases"][0], "http://127.0.0.1:3000/users/alice");
    assert_eq!(json["links"][0]["rel"], "self");
    assert_eq!(json["links"][0]["type"], "application/activity+json");
    assert_eq!(
        json["links"][0]["href"],
        "http://127.0.0.1:3000/users/alice"
    );
}

#[tokio::test]
async fn rejects_missing_resource() {
    let app = test_router(test_config()).expect("build router");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/.well-known/webfinger")
                .body(Body::empty())
                .expect("valid request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rejects_non_local_actor_resource() {
    let app = test_router(test_config()).expect("build router");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/.well-known/webfinger?resource=acct:bob@127.0.0.1:3000")
                .body(Body::empty())
                .expect("valid request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
