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

pub async fn note(
    State(app_state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, StatusCode> {
    // TODO(#25): Replace this seeded preview note with durable runtime storage.
    let note = app_state
        .notes
        .iter()
        .find(|note| note.id.as_str() == format!("http://{}/notes/{id}", app_state.handle_host))
        .cloned()
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok((
        [(header::CONTENT_TYPE, "application/activity+json")],
        Json(note),
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

    use crate::{app::build_router, config::RuntimeConfig};

    #[tokio::test]
    async fn returns_public_note() {
        let app = build_router(&RuntimeConfig::default_local());

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/notes/1")
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
        assert_eq!(json["type"], "Note");
        assert_eq!(json["id"], "http://127.0.0.1:3000/notes/1");
        assert_eq!(json["attributedTo"], "http://127.0.0.1:3000/users/alice");
        assert_eq!(
            json["content"],
            "Hello, World! This is Feder, a portable AP core for many runtimes."
        );
    }

    #[tokio::test]
    async fn rejects_unknown_note() {
        let app = build_router(&RuntimeConfig::default_local());

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/notes/missing")
                    .body(Body::empty())
                    .expect("valid request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
