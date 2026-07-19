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
use feder_core::{Action, Activity, SendActivity};
use feder_vocab::{Create, Note, Reference};

use crate::common::{spawn_inbox_server, test_activity_sender};

fn create_note_send_action(inbox: &str) -> Action {
    let actor_id = "https://local.example/users/alice"
        .parse()
        .expect("valid actor IRI");
    let note = Note::new(
        "https://local.example/notes/1"
            .parse()
            .expect("valid note IRI"),
    );
    let create = Create::new(
        "https://local.example/activities/create-1"
            .parse()
            .expect("valid activity IRI"),
        Reference::id(actor_id),
        Reference::object(note),
    );

    Action::SendActivity(SendActivity {
        activity: Activity::CreateNote(create),
        inbox: inbox.parse().expect("valid inbox IRI"),
    })
}

#[tokio::test]
async fn sends_create_note_action() {
    let (inbox, mut requests, inbox_server) = spawn_inbox_server(StatusCode::ACCEPTED).await;
    let actions = [create_note_send_action(&inbox)];

    test_activity_sender()
        .send_actions(&actions)
        .await
        .expect("send Create activity");

    let request = requests.recv().await.expect("receive Create request");
    assert_eq!(
        request.headers["host"],
        inbox
            .strip_prefix("http://")
            .and_then(|value| value.strip_suffix("/inbox"))
            .expect("inbox authority")
    );
    assert!(httpdate::parse_http_date(request.headers["date"].to_str().unwrap()).is_ok());
    assert_eq!(
        request.headers["digest"],
        feder_core::http_signatures::create_sha256_digest_header(&request.body)
    );
    let signature = request.headers["signature"].to_str().unwrap();
    assert!(signature.starts_with(
        "keyId=\"https://local.example/users/alice#main-key\",algorithm=\"rsa-sha256\",headers=\"(request-target) content-type date digest host\",signature=\""
    ));
    let activity: serde_json::Value =
        serde_json::from_slice(&request.body).expect("valid sent activity");
    assert_eq!(activity["type"], "Create");
    assert_eq!(activity["actor"], "https://local.example/users/alice");
    assert_eq!(activity["object"]["type"], "Note");
    inbox_server.abort();
}

#[tokio::test]
async fn attempts_later_sends_after_failure() {
    let (failed_inbox, mut failed_requests, failed_server) =
        spawn_inbox_server(StatusCode::INTERNAL_SERVER_ERROR).await;
    let (successful_inbox, mut successful_requests, successful_server) =
        spawn_inbox_server(StatusCode::ACCEPTED).await;
    let actions = [
        create_note_send_action(&failed_inbox),
        create_note_send_action(&successful_inbox),
    ];

    let result = test_activity_sender().send_actions(&actions).await;

    assert!(result.is_err());
    failed_requests
        .recv()
        .await
        .expect("receive failed request");
    successful_requests
        .recv()
        .await
        .expect("receive later request");
    failed_server.abort();
    successful_server.abort();
}
