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
    Json,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};

use crate::app::AppState;

pub async fn actor(
    State(app_state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Response, StatusCode> {
    if username != app_state.username {
        return Err(StatusCode::NOT_FOUND);
    }
    let local_actor = app_state.local_actor.clone();

    Ok((
        [(header::CONTENT_TYPE, "application/activity+json")],
        Json(local_actor),
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode, header},
    };
    use serde_json::Value;
    use tower::ServiceExt;

    use crate::{build_router, config::test_config};

    #[tokio::test]
    async fn returns_local_actor() {
        let app = build_router(test_config());

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
        let app = build_router(test_config());

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
}
