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
    extract::{Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

use crate::app::AppState;

#[derive(Deserialize)]
pub struct WebFingerQuery {
    resource: Option<String>,
}

#[derive(Serialize)]
pub struct WebFingerResponse {
    subject: String,
    aliases: Vec<String>,
    links: Vec<WebFingerLink>,
}

#[derive(Serialize)]
pub struct WebFingerLink {
    rel: &'static str,
    #[serde(rename = "type")]
    media_type: &'static str,
    href: String,
}

pub async fn webfinger(
    State(state): State<AppState>,
    Query(query): Query<WebFingerQuery>,
) -> Result<Response, StatusCode> {
    let Some(resource) = query.resource else {
        return Err(StatusCode::BAD_REQUEST);
    };

    let expected = format!("acct:{}@{}", state.username, state.handle_host);
    if resource != expected {
        return Err(StatusCode::NOT_FOUND);
    }

    let actor_id = state.actor_id.to_string();

    Ok((
        [(header::CONTENT_TYPE, "application/jrd+json")],
        Json(WebFingerResponse {
            subject: resource,
            aliases: vec![actor_id.clone()],
            links: vec![WebFingerLink {
                rel: "self",
                media_type: "application/activity+json",
                href: actor_id,
            }],
        }),
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

    const WEBFINGER_PATH: &str = "/.well-known/webfinger?resource=acct:alice@127.0.0.1:3000";

    #[tokio::test]
    async fn returns_webfinger_descriptor_for_local_actor() {
        let app = build_router(&RuntimeConfig::default_local());

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
        let app = build_router(&RuntimeConfig::default_local());

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
        let app = build_router(&RuntimeConfig::default_local());

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
}
