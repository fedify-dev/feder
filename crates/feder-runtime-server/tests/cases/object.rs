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
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use feder_core::{Action, Object, PUBLIC_COLLECTION, StoreObject};
use feder_runtime_server::{app::router_with_state, config::StorageConfig, storage::RuntimeStore};
use feder_vocab::{Iri, Note, Reference, References};
use serde_json::Value;
use tower::ServiceExt;

use crate::common::{temporary_database_path, test_app_state, test_config, test_router};

fn iri(value: &str) -> Iri {
    value.parse().expect("valid test IRI")
}

fn stored_note() -> Note {
    let mut note = Note::new(iri("http://127.0.0.1:3000/users/alice/posts/1"));
    note.attributed_to = Some(Reference::id(iri("http://127.0.0.1:3000/users/alice")));
    note.to = References::one(iri(PUBLIC_COLLECTION));
    note.cc = References::one(iri("http://127.0.0.1:3000/users/alice/followers"));
    note.content = Some("Hello from Feder.".to_string());
    note.media_type = Some("text/html".to_string());
    note.published = Some("2026-07-21T00:00:00Z".to_string());
    note.url = Some(note.id.clone());
    note
}

fn router_with_stored_note(note: Note) -> Router {
    let state = test_app_state(test_config()).expect("build app state");
    state
        .store
        .lock()
        .expect("store lock")
        .persist_actions(&[Action::StoreObject(StoreObject {
            object: Object::Note(note),
        })])
        .expect("persist note");
    router_with_state(state)
}

fn router_with_note() -> Router {
    router_with_stored_note(stored_note())
}

async fn get_object(app: Router, uri: &str, accept: Option<&str>) -> axum::response::Response {
    let mut request = Request::builder().uri(uri);
    if let Some(accept) = accept {
        request = request.header(header::ACCEPT, accept);
    }

    app.oneshot(request.body(Body::empty()).expect("valid request"))
        .await
        .expect("response")
}

#[tokio::test]
async fn returns_stored_note_with_activitypub_headers() {
    let response = get_object(
        router_with_note(),
        "/users/alice/posts/1",
        Some("application/activity+json"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/activity+json"
    );
    assert_eq!(response.headers().get(header::VARY).unwrap(), "Accept");

    let body = to_bytes(response.into_body(), 4096)
        .await
        .expect("read response body");
    let json: Value = serde_json::from_slice(&body).expect("valid JSON");
    assert_eq!(json["type"], "Note");
    assert_eq!(json["id"], "http://127.0.0.1:3000/users/alice/posts/1");
    assert_eq!(json["content"], "Hello from Feder.");
    assert_eq!(json["mediaType"], "text/html");
}

#[tokio::test]
async fn returns_note_when_public_is_in_cc() {
    let mut note = stored_note();
    note.to = References::one(iri("https://remote.example/users/bob"));
    note.cc = References::one(iri(PUBLIC_COLLECTION));

    let response = get_object(
        router_with_stored_note(note),
        "/users/alice/posts/1",
        Some("application/activity+json"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn returns_not_found_for_direct_note() {
    let mut note = stored_note();
    note.to = References::one(iri("https://remote.example/users/bob"));
    note.cc = References::new();

    let response = get_object(
        router_with_stored_note(note),
        "/users/alice/posts/1",
        Some("application/activity+json"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn returns_not_found_for_followers_only_note() {
    let mut note = stored_note();
    note.to = References::one(iri("http://127.0.0.1:3000/users/alice/followers"));
    note.cc = References::new();

    let response = get_object(
        router_with_stored_note(note),
        "/users/alice/posts/1",
        Some("application/activity+json"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn returns_not_found_for_note_without_audience() {
    let mut note = stored_note();
    note.to = References::new();
    note.cc = References::new();

    let response = get_object(
        router_with_stored_note(note),
        "/users/alice/posts/1",
        Some("application/activity+json"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn rejects_note_request_when_html_is_preferred() {
    let response = get_object(
        router_with_note(),
        "/users/alice/posts/1",
        Some("text/html, application/activity+json;q=0.8"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_ACCEPTABLE);
    assert_eq!(response.headers().get(header::VARY).unwrap(), "Accept");
}

#[tokio::test]
async fn rejects_note_request_without_activitypub_accept() {
    let response = get_object(router_with_note(), "/users/alice/posts/1", None).await;

    assert_eq!(response.status(), StatusCode::NOT_ACCEPTABLE);
    assert_eq!(response.headers().get(header::VARY).unwrap(), "Accept");
}

#[tokio::test]
async fn returns_not_found_for_unknown_note() {
    let response = get_object(
        router_with_note(),
        "/users/alice/posts/unknown",
        Some("text/html"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn returns_note_after_store_reopen() {
    let path = temporary_database_path("feder-object-route-test");
    {
        let mut config = test_config();
        config.storage = StorageConfig::Sqlite { path: path.clone() };
        let state = test_app_state(config).expect("build app state");
        state
            .store
            .lock()
            .expect("store lock")
            .persist_actions(&[Action::StoreObject(StoreObject {
                object: Object::Note(stored_note()),
            })])
            .expect("persist note");
    }

    let mut config = test_config();
    config.storage = StorageConfig::Sqlite { path: path.clone() };
    let response = get_object(
        test_router(config).expect("reopen router"),
        "/users/alice/posts/1",
        Some("application/activity+json"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);

    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn returns_not_found_for_unknown_username() {
    let response = get_object(
        router_with_note(),
        "/users/bob/posts/1",
        Some("application/activity+json"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
