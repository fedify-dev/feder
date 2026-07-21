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

use axum::http::StatusCode;
use feder_core::{Action, Input, Object, UserCreateNote};
use feder_runtime_server::{Error, send::SendError, storage::RuntimeStore};
use feder_vocab::{Actor, Follow, Iri, Reference};

use crate::common::{spawn_inbox_server, test_app_state, test_config};

fn iri(value: &str) -> Iri {
    value.parse().expect("valid test IRI")
}

fn create_note_input() -> UserCreateNote {
    UserCreateNote {
        note_id: iri("http://127.0.0.1:3000/users/alice/posts/1"),
        create_id: iri("http://127.0.0.1:3000/users/alice/activities/create/1"),
        actor: Reference::id(iri("http://127.0.0.1:3000/users/alice")),
        content: "Hello from Feder.".to_string(),
        published: Some("2026-07-21T00:00:00Z".to_string()),
    }
}

fn add_delivery_target(state: &feder_runtime_server::AppState, inbox: &str) {
    let remote_actor_id = iri("https://remote.example/users/bob");
    let remote_actor = Actor::person(
        remote_actor_id.clone(),
        iri(inbox),
        iri("https://remote.example/users/bob/outbox"),
    );
    let follow = Follow::new(
        iri("https://remote.example/activities/follow/1"),
        Reference::object(remote_actor),
        Reference::id(state.local_actor.id.clone()),
    );

    let _ = state
        .core
        .lock()
        .expect("core lock")
        .handle(Input::received_follow(
            follow,
            iri("http://127.0.0.1:3000/users/alice/activities/accept/1"),
        ));
}

#[tokio::test]
async fn create_note_persists_and_delivers_the_core_actions() {
    let (inbox, mut requests, inbox_server) = spawn_inbox_server(StatusCode::ACCEPTED).await;
    let state = test_app_state(test_config()).expect("build app state");
    add_delivery_target(&state, &inbox);

    let result = state
        .create_note(create_note_input())
        .await
        .expect("create note");

    assert_eq!(result.actions.len(), 2);
    assert!(matches!(result.actions[0], Action::StoreObject(_)));
    assert!(matches!(result.actions[1], Action::SendActivity(_)));

    let stored = state
        .store
        .lock()
        .expect("store lock")
        .load_object(&iri("http://127.0.0.1:3000/users/alice/posts/1"))
        .expect("load note")
        .expect("stored note");
    let Object::Note(note) = stored else {
        panic!("expected stored Note");
    };
    assert_eq!(note.content.as_deref(), Some("Hello from Feder."));

    let request = requests.recv().await.expect("receive Create request");
    let activity: serde_json::Value =
        serde_json::from_slice(&request.body).expect("valid Create activity");
    assert_eq!(activity["type"], "Create");
    assert_eq!(activity["object"]["id"], note.id.as_str());
    inbox_server.abort();
}

#[tokio::test]
async fn create_note_keeps_the_persisted_object_when_delivery_fails() {
    let (inbox, mut requests, inbox_server) =
        spawn_inbox_server(StatusCode::INTERNAL_SERVER_ERROR).await;
    let state = test_app_state(test_config()).expect("build app state");
    add_delivery_target(&state, &inbox);

    let result = state.create_note(create_note_input()).await;

    assert!(matches!(
        result,
        Err(Error::ActivitySender(SendError::UnsuccessfulStatus { .. }))
    ));
    requests.recv().await.expect("receive Create request");
    assert!(
        state
            .store
            .lock()
            .expect("store lock")
            .load_object(&iri("http://127.0.0.1:3000/users/alice/posts/1"))
            .expect("load note")
            .is_some()
    );
    inbox_server.abort();
}
