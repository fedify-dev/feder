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
use feder_core::{Action, Object, Recipients, StoreFollower, UserCreateNote};
use feder_runtime_server::{Error, config::StorageConfig, send::SendError, storage::RuntimeStore};
use feder_vocab::{Actor, Iri, Reference};

use crate::common::{spawn_inbox_server, temporary_database_path, test_app_state, test_config};

fn iri(value: &str) -> Iri {
    value.parse().expect("valid test IRI")
}

fn create_note_input() -> UserCreateNote {
    UserCreateNote {
        note_id: iri("http://127.0.0.1:3000/users/alice/posts/1"),
        create_id: iri("http://127.0.0.1:3000/users/alice/activities/create/1"),
        actor: Reference::id(iri("http://127.0.0.1:3000/users/alice")),
        to: feder_vocab::References::one(iri("https://www.w3.org/ns/activitystreams#Public")),
        cc: feder_vocab::References::one(iri("http://127.0.0.1:3000/users/alice/followers")),
        content: "Hello from Feder.".to_string(),
        media_type: Some("text/html".to_string()),
        published: Some("2026-07-21T00:00:00Z".to_string()),
        url: Some(iri("http://127.0.0.1:3000/@alice/1")),
    }
}

fn store_follower(state: &feder_runtime_server::AppState, remote_actor_id: &str, inbox: &str) {
    let remote_actor_id = iri(remote_actor_id);
    let remote_actor = Actor::person(
        remote_actor_id.clone(),
        iri(inbox),
        iri(&format!("{remote_actor_id}/outbox")),
    );
    state
        .store
        .lock()
        .expect("store lock")
        .persist_actions(&[Action::StoreFollower(StoreFollower {
            follower: Reference::object(remote_actor),
            following: Reference::id(state.local_actor.id.clone()),
        })])
        .expect("persist follower");
}

#[tokio::test]
async fn create_note_persists_and_delivers_the_core_actions() {
    let (inbox, mut requests, inbox_server) = spawn_inbox_server(StatusCode::ACCEPTED).await;
    let state = test_app_state(test_config()).expect("build app state");
    store_follower(&state, "https://remote.example/users/bob", &inbox);

    let result = state
        .create_note(create_note_input())
        .await
        .expect("create note");

    assert_eq!(result.actions.len(), 2);
    assert!(matches!(result.actions[0], Action::StoreObject(_)));
    assert!(matches!(
        &result.actions[1],
        Action::SendActivity(send)
            if matches!(&send.recipients, Recipients::Followers(_))
    ));

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
    assert_eq!(
        activity["to"],
        "https://www.w3.org/ns/activitystreams#Public"
    );
    assert_eq!(
        activity["cc"],
        "http://127.0.0.1:3000/users/alice/followers"
    );
    assert_eq!(activity["object"]["id"], note.id.as_str());
    assert_eq!(activity["object"]["to"], activity["to"]);
    assert_eq!(activity["object"]["cc"], activity["cc"]);
    assert_eq!(activity["object"]["mediaType"], "text/html");
    assert_eq!(activity["object"]["url"], "http://127.0.0.1:3000/@alice/1");
    inbox_server.abort();
}

#[tokio::test]
async fn create_note_delivers_to_each_persisted_follower() {
    let (bob_inbox, mut bob_requests, bob_server) = spawn_inbox_server(StatusCode::ACCEPTED).await;
    let (carol_inbox, mut carol_requests, carol_server) =
        spawn_inbox_server(StatusCode::ACCEPTED).await;
    let state = test_app_state(test_config()).expect("build app state");
    store_follower(&state, "https://remote.example/users/bob", &bob_inbox);
    store_follower(&state, "https://another.example/users/carol", &carol_inbox);

    state
        .create_note(create_note_input())
        .await
        .expect("create note");

    bob_requests.recv().await.expect("receive Bob delivery");
    carol_requests.recv().await.expect("receive Carol delivery");
    bob_server.abort();
    carol_server.abort();
}

#[tokio::test]
async fn create_note_keeps_the_persisted_object_when_delivery_fails() {
    let (inbox, mut requests, inbox_server) =
        spawn_inbox_server(StatusCode::INTERNAL_SERVER_ERROR).await;
    let state = test_app_state(test_config()).expect("build app state");
    store_follower(&state, "https://remote.example/users/bob", &inbox);

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

#[tokio::test]
async fn create_note_resolves_persisted_followers_after_restart() {
    let (inbox, mut requests, inbox_server) = spawn_inbox_server(StatusCode::ACCEPTED).await;
    let path = temporary_database_path("feder-create-note-recipients-test");
    let mut first_config = test_config();
    first_config.storage = StorageConfig::Sqlite { path: path.clone() };
    {
        let state = test_app_state(first_config).expect("build app state");
        store_follower(&state, "https://remote.example/users/bob", &inbox);
    }

    let mut second_config = test_config();
    second_config.storage = StorageConfig::Sqlite { path: path.clone() };
    let state = test_app_state(second_config).expect("reopen app state");
    assert!(
        state
            .core
            .lock()
            .expect("core lock")
            .state()
            .followers()
            .is_empty()
    );

    state
        .create_note(create_note_input())
        .await
        .expect("create note after restart");

    requests.recv().await.expect("receive Create request");
    inbox_server.abort();
    let _ = std::fs::remove_file(path);
}
