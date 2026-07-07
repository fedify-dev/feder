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
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, Method, StatusCode, Uri, header::CONTENT_TYPE},
    response::{IntoResponse, Response},
};

use feder_core::Input;
use feder_vocab::Follow;
use serde_json::{Value, from_slice, from_value};

use crate::app::AppState;

pub struct InboxRequest {
    pub username: String,
    pub headers: HeaderMap,
    pub method: Method,
    pub uri: Uri,
    pub body: Bytes,
}

fn accept_id_for_follow(
    local_actor_id: &feder_vocab::Iri,
    follow_id: &feder_vocab::Iri,
) -> Result<feder_vocab::Iri, StatusCode> {
    let encoded_follow_id = percent_encoding::utf8_percent_encode(
        follow_id.as_str(),
        percent_encoding::NON_ALPHANUMERIC,
    );

    format!("{local_actor_id}#accepts/{encoded_follow_id}")
        .parse()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

pub async fn inbox(
    State(app_state): State<AppState>,
    Path(username): Path<String>,
    headers: HeaderMap,
    method: Method,
    uri: Uri,
    body: Bytes,
) -> Result<Response, StatusCode> {
    if username != app_state.username {
        return Err(StatusCode::NOT_FOUND);
    }
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");

    if !content_type.starts_with("application/activity+json")
        && !content_type.starts_with("application/ld+json")
    {
        return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    let req = InboxRequest {
        username,
        headers,
        method,
        uri,
        body,
    };

    let value: Value = from_slice(&req.body).map_err(|_| StatusCode::BAD_REQUEST)?;

    let activity_type = value.get("type").and_then(|value| value.as_str());

    // Unsupported activity types will be ignored
    if activity_type != Some("Follow") {
        return Ok(StatusCode::ACCEPTED.into_response());
    }
    let follow: Follow = from_value(value).map_err(|_| StatusCode::BAD_REQUEST)?;
    let accept_id = accept_id_for_follow(&app_state.local_actor.id, &follow.id)?;
    let input = Input::received_follow(follow, accept_id);

    let mut core = app_state
        .core
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let _result = core.handle(input);

    Ok(StatusCode::ACCEPTED.into_response())
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{Body, Bytes},
        extract::Path,
        http::{HeaderMap, Method, Request, StatusCode, Uri, header::CONTENT_TYPE},
        response::Response,
    };
    use serde_json::json;
    use tower::ServiceExt;

    use crate::{app::AppState, build_router, config::test_config};

    use super::inbox;

    fn activity_json_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, "application/activity+json".parse().unwrap());
        headers
    }

    fn follow_body() -> Bytes {
        Bytes::from(
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
            .expect("serialize follow"),
        )
    }

    async fn post_inbox(
        app_state: AppState,
        username: &str,
        headers: HeaderMap,
        body: Bytes,
    ) -> Result<Response, StatusCode> {
        inbox(
            axum::extract::State(app_state),
            Path(username.to_string()),
            headers,
            Method::POST,
            Uri::from_static("/users/alice/inbox"),
            body,
        )
        .await
    }

    #[tokio::test]
    async fn valid_follow_reaches_core() {
        let app_state = AppState::from_config(test_config());

        let response = post_inbox(
            app_state.clone(),
            "alice",
            activity_json_headers(),
            follow_body(),
        )
        .await
        .expect("accepted follow");

        assert_eq!(response.status(), StatusCode::ACCEPTED);

        let core = app_state.core.lock().expect("core lock");
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
    async fn rejects_unknown_inbox_actor() {
        let app_state = AppState::from_config(test_config());

        let error = post_inbox(
            app_state.clone(),
            "bob",
            activity_json_headers(),
            follow_body(),
        )
        .await
        .expect_err("unknown inbox actor should be rejected");

        assert_eq!(error, StatusCode::NOT_FOUND);
        assert!(
            app_state
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
        let app_state = AppState::from_config(test_config());
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, "application/json".parse().unwrap());

        let error = post_inbox(app_state.clone(), "alice", headers, follow_body())
            .await
            .expect_err("unsupported content type should be rejected");

        assert_eq!(error, StatusCode::UNSUPPORTED_MEDIA_TYPE);
        assert!(
            app_state
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
        let app_state = AppState::from_config(test_config());

        let error = post_inbox(
            app_state.clone(),
            "alice",
            activity_json_headers(),
            Bytes::from_static(b"{not json"),
        )
        .await
        .expect_err("malformed json should be rejected");

        assert_eq!(error, StatusCode::BAD_REQUEST);
        assert!(
            app_state
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
        let app_state = AppState::from_config(test_config());
        let body = Bytes::from(
            serde_json::to_vec(&json!({
                "@context": "https://www.w3.org/ns/activitystreams",
                "type": "Create",
                "id": "https://remote.example/activities/create-1",
                "actor": "https://remote.example/users/bob",
                "object": {
                    "type": "Note",
                    "id": "https://remote.example/notes/1"
                }
            }))
            .expect("serialize create"),
        );

        let response = post_inbox(app_state.clone(), "alice", activity_json_headers(), body)
            .await
            .expect("unsupported activity is accepted but ignored");

        assert_eq!(response.status(), StatusCode::ACCEPTED);
        assert!(
            app_state
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
        let app = build_router(test_config());
        let oversized_body = vec![b' '; 1_048_577];

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/users/alice/inbox")
                    .header(CONTENT_TYPE, "application/activity+json")
                    .body(Body::from(oversized_body))
                    .expect("valid request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }
}
