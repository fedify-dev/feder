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
use feder_core::{
    Activity, Recipients, SendActivity,
    http_signatures::{ActorKeyPair, sign_draft_cavage},
};
use feder_runtime_server::{OutboundAddressPolicy, send::SendError};
use feder_vocab::{Create, Follow, Note, Reference};

use crate::common::{spawn_inbox_server, test_activity_sender, test_activity_sender_with_policy};

fn create_note_send_action(inbox: &str) -> SendActivity {
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

    SendActivity {
        activity: Activity::CreateNote(create),
        recipients: Recipients::Inbox(inbox.parse().expect("valid inbox IRI")),
    }
}

fn follow_send_action(inbox: &str) -> SendActivity {
    let follow = Follow::new(
        "https://local.example/activities/follow-1"
            .parse()
            .expect("valid activity IRI"),
        Reference::id(
            "https://local.example/users/alice"
                .parse()
                .expect("valid actor IRI"),
        ),
        Reference::id(
            "https://remote.example/users/bob"
                .parse()
                .expect("valid actor IRI"),
        ),
    );

    SendActivity {
        activity: Activity::Follow(follow),
        recipients: Recipients::Inbox(inbox.parse().expect("valid inbox IRI")),
    }
}

#[tokio::test]
async fn sends_create_note_action() {
    let (inbox, mut requests, inbox_server) = spawn_inbox_server(StatusCode::ACCEPTED).await;
    let inbox = format!("{inbox}?shared=true");
    let actions = [create_note_send_action(&inbox)];

    test_activity_sender()
        .send_actions(&actions)
        .await
        .expect("send Create activity");

    let request = requests.recv().await.expect("receive Create request");
    assert_eq!(request.uri, "/inbox?shared=true");
    assert_eq!(
        request.headers["host"],
        inbox
            .strip_prefix("http://")
            .and_then(|value| value.split_once('/').map(|(authority, _)| authority))
            .expect("inbox authority")
    );
    assert!(httpdate::parse_http_date(request.headers["date"].to_str().unwrap()).is_ok());
    assert_eq!(
        request.headers["digest"],
        feder_core::http_signatures::create_sha256_digest_header(&request.body)
    );
    let signature = request.headers["signature"].to_str().unwrap();
    let headers = [
        (
            "content-type",
            request.headers["content-type"].to_str().unwrap(),
        ),
        ("date", request.headers["date"].to_str().unwrap()),
        ("digest", request.headers["digest"].to_str().unwrap()),
        ("host", request.headers["host"].to_str().unwrap()),
    ];
    let key_pair = ActorKeyPair::from_pem(
        include_str!("../fixtures/rsa-private-key.pem").to_string(),
        include_str!("../fixtures/rsa-public-key.pem").to_string(),
    )
    .expect("load actor key pair fixture");
    let expected_signature = sign_draft_cavage(
        &key_pair,
        "https://local.example/users/alice#main-key",
        "POST",
        "/inbox?shared=true",
        &headers,
    )
    .expect("sign captured request");
    assert_eq!(signature, expected_signature);
    let activity: serde_json::Value =
        serde_json::from_slice(&request.body).expect("valid sent activity");
    assert_eq!(activity["type"], "Create");
    assert_eq!(activity["actor"], "https://local.example/users/alice");
    assert_eq!(activity["object"]["type"], "Note");
    inbox_server.abort();
}

#[tokio::test]
async fn sends_follow_action() {
    let (inbox, mut requests, inbox_server) = spawn_inbox_server(StatusCode::ACCEPTED).await;

    test_activity_sender()
        .send_actions(&[follow_send_action(&inbox)])
        .await
        .expect("send Follow activity");

    let request = requests.recv().await.expect("receive Follow request");
    assert_eq!(request.uri, "/inbox");
    assert_eq!(request.headers["content-type"], "application/activity+json");
    assert!(request.headers.contains_key("signature"));
    let activity: serde_json::Value =
        serde_json::from_slice(&request.body).expect("valid sent activity");
    assert_eq!(activity["type"], "Follow");
    assert_eq!(activity["actor"], "https://local.example/users/alice");
    assert_eq!(activity["object"], "https://remote.example/users/bob");
    inbox_server.abort();
}

#[tokio::test]
async fn rejects_unresolved_follower_recipients() {
    let mut action = create_note_send_action("https://remote.example/inbox");
    action.recipients = Recipients::Followers(
        "https://local.example/users/alice"
            .parse()
            .expect("valid actor IRI"),
    );

    let result = test_activity_sender().send_actions(&[action]).await;

    assert!(matches!(result, Err(SendError::UnresolvedRecipients)));
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

#[tokio::test]
async fn blocks_literal_private_inbox_address() {
    let (inbox, mut requests, inbox_server) = spawn_inbox_server(StatusCode::ACCEPTED).await;
    let actions = [create_note_send_action(&inbox)];

    let result = test_activity_sender_with_policy(OutboundAddressPolicy::PublicOnly)
        .send_actions(&actions)
        .await;

    assert!(matches!(
        result,
        Err(SendError::PrivateInboxAddress { address, .. }) if address.is_loopback()
    ));
    assert!(requests.try_recv().is_err());
    inbox_server.abort();
}

#[tokio::test]
async fn blocks_hostname_resolving_to_private_address() {
    let (inbox, mut requests, inbox_server) = spawn_inbox_server(StatusCode::ACCEPTED).await;
    let inbox = inbox.replacen("127.0.0.1", "localhost", 1);
    let actions = [create_note_send_action(&inbox)];

    let result = test_activity_sender_with_policy(OutboundAddressPolicy::PublicOnly)
        .send_actions(&actions)
        .await;

    assert!(matches!(result, Err(SendError::Request(_))));
    assert!(requests.try_recv().is_err());
    inbox_server.abort();
}

#[tokio::test]
async fn blocks_special_use_ipv6_inbox_addresses() {
    let sender = test_activity_sender_with_policy(OutboundAddressPolicy::PublicOnly);

    for address in ["100:0:0:1::1", "2001:2::1", "5f00::1"] {
        let inbox = format!("http://[{address}]/inbox");
        let actions = [create_note_send_action(&inbox)];

        let result = sender.send_actions(&actions).await;

        assert!(matches!(result, Err(SendError::PrivateInboxAddress { .. })));
    }
}
