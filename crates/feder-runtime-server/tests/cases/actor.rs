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

    let body = to_bytes(response.into_body(), 2048)
        .await
        .expect("read response body");
    let json: Value = serde_json::from_slice(&body).expect("valid json");

    assert_eq!(json["@context"], "https://www.w3.org/ns/activitystreams");
    assert_eq!(json["type"], "Person");
    assert_eq!(json["id"], "http://127.0.0.1:3000/users/alice");
    assert_eq!(json["inbox"], "http://127.0.0.1:3000/users/alice/inbox");
    assert_eq!(json["outbox"], "http://127.0.0.1:3000/users/alice/outbox");
    assert_eq!(json["preferredUsername"], "alice");
    assert_eq!(json["name"], "alice");
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
