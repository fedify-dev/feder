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

use feder_vocab::{
    ACTIVITYSTREAMS_CONTEXT, Accept, Actor, Create, CryptographicKey, Follow, Iri, Note,
    OrderedCollection, Reference, References, SECURITY_CONTEXT, Undo,
};
use serde_json::{Value, json};

fn serialize_to_value(value: impl serde::Serialize) -> Value {
    serde_json::to_value(value).expect("serialize vocab value")
}

fn iri(value: &str) -> Iri {
    value.parse().expect("valid test IRI")
}

fn incoming_follow_json() -> serde_json::Value {
    json!({
        "@context": ACTIVITYSTREAMS_CONTEXT,
        "type": "Follow",
        "id": "https://remote.example/activities/follow/1",
        "actor": "https://remote.example/users/bob",
        "object": {
            "type": "Person",
            "id": "https://example.com/users/alice",
            "inbox": "https://example.com/users/alice/inbox",
            "outbox": "https://example.com/users/alice/outbox",
            "preferredUsername": "alice"
        }
    })
}

#[test]
fn actor_serializes_embedded_cryptographic_key() {
    let actor_id = iri("https://example.com/users/alice");
    let mut actor = Actor::person(
        actor_id.clone(),
        iri("https://example.com/users/alice/inbox"),
        iri("https://example.com/users/alice/outbox"),
    );
    actor.set_public_key(Reference::object(CryptographicKey::new(
        iri("https://example.com/users/alice#main-key"),
        actor_id,
        "-----BEGIN PUBLIC KEY-----\ntest\n-----END PUBLIC KEY-----\n".to_string(),
    )));

    assert_eq!(
        serialize_to_value(actor),
        json!({
            "@context": [ACTIVITYSTREAMS_CONTEXT, SECURITY_CONTEXT],
            "type": "Person",
            "id": "https://example.com/users/alice",
            "inbox": "https://example.com/users/alice/inbox",
            "outbox": "https://example.com/users/alice/outbox",
            "publicKey": {
                "id": "https://example.com/users/alice#main-key",
                "type": "CryptographicKey",
                "owner": "https://example.com/users/alice",
                "publicKeyPem": "-----BEGIN PUBLIC KEY-----\ntest\n-----END PUBLIC KEY-----\n"
            }
        })
    );
}

#[test]
fn follow_activity_accepts_id_or_embedded_actor_references() {
    let follow: Follow =
        serde_json::from_value(incoming_follow_json()).expect("deserialize incoming follow");

    assert_eq!(follow.id, iri("https://remote.example/activities/follow/1"));
    assert!(
        matches!(follow.actor, Reference::Id(id) if id == iri("https://remote.example/users/bob"))
    );
    assert!(
        matches!(follow.object, Reference::Object(actor) if actor.id == iri("https://example.com/users/alice"))
    );
}

#[test]
fn accept_activity_can_embed_follow_activity() {
    let follow: Follow =
        serde_json::from_value(incoming_follow_json()).expect("deserialize incoming follow");

    let outgoing_accept = Accept::new(
        iri("https://example.com/activities/accept/1"),
        Reference::id(iri("https://example.com/users/alice")),
        Reference::object(follow),
    );

    assert_eq!(
        serialize_to_value(outgoing_accept),
        json!({
            "@context": ACTIVITYSTREAMS_CONTEXT,
            "type": "Accept",
            "id": "https://example.com/activities/accept/1",
            "actor": "https://example.com/users/alice",
            "object": {
                "@context": ACTIVITYSTREAMS_CONTEXT,
                "type": "Follow",
                "id": "https://remote.example/activities/follow/1",
                "actor": "https://remote.example/users/bob",
                "object": {
                    "type": "Person",
                    "id": "https://example.com/users/alice",
                    "inbox": "https://example.com/users/alice/inbox",
                    "outbox": "https://example.com/users/alice/outbox",
                    "preferredUsername": "alice"
                }
            }
        })
    );
}

#[test]
fn undo_activity_can_embed_follow_activity() {
    let follow: Follow =
        serde_json::from_value(incoming_follow_json()).expect("deserialize incoming follow");
    let undo = Undo::new(
        iri("https://remote.example/activities/undo/1"),
        Reference::id(iri("https://remote.example/users/bob")),
        Reference::object(follow),
    );

    assert_eq!(
        serialize_to_value(undo),
        json!({
            "@context": ACTIVITYSTREAMS_CONTEXT,
            "type": "Undo",
            "id": "https://remote.example/activities/undo/1",
            "actor": "https://remote.example/users/bob",
            "object": incoming_follow_json()
        })
    );
}

#[test]
fn ordered_collection_serializes_actor_iris() {
    let collection = OrderedCollection::new(
        iri("https://example.com/users/alice/followers"),
        2,
        vec![
            iri("https://remote.example/users/bob"),
            iri("https://another.example/users/carol"),
        ],
    );

    assert_eq!(
        serialize_to_value(collection),
        json!({
            "@context": ACTIVITYSTREAMS_CONTEXT,
            "type": "OrderedCollection",
            "id": "https://example.com/users/alice/followers",
            "totalItems": 2,
            "orderedItems": [
                "https://remote.example/users/bob",
                "https://another.example/users/carol"
            ]
        })
    );
}

#[test]
fn local_note_serializes_as_create_activity() {
    let mut note = Note::new(iri("https://example.com/notes/1"));
    note.attributed_to = Some(Reference::id(iri("https://example.com/users/alice")));
    note.to = References::one(iri("https://www.w3.org/ns/activitystreams#Public"));
    note.cc = References::one(iri("https://example.com/users/alice/followers"));
    note.content = Some("Hello from Feder.".to_string());
    note.media_type = Some("text/html".to_string());
    note.published = Some("2026-06-02T00:00:00Z".to_string());
    note.url = Some(iri("https://example.com/@alice/1"));

    let mut create = Create::new(
        iri("https://example.com/activities/create/1"),
        Reference::id(iri("https://example.com/users/alice")),
        Reference::object(note),
    );
    create.to = References::one(iri("https://www.w3.org/ns/activitystreams#Public"));
    create.cc = References::one(iri("https://example.com/users/alice/followers"));

    assert_eq!(
        serialize_to_value(create),
        json!({
            "@context": ACTIVITYSTREAMS_CONTEXT,
            "type": "Create",
            "id": "https://example.com/activities/create/1",
            "actor": "https://example.com/users/alice",
            "to": "https://www.w3.org/ns/activitystreams#Public",
            "cc": "https://example.com/users/alice/followers",
            "object": {
                "@context": ACTIVITYSTREAMS_CONTEXT,
                "type": "Note",
                "id": "https://example.com/notes/1",
                "attributedTo": "https://example.com/users/alice",
                "to": "https://www.w3.org/ns/activitystreams#Public",
                "cc": "https://example.com/users/alice/followers",
                "content": "Hello from Feder.",
                "mediaType": "text/html",
                "published": "2026-06-02T00:00:00Z",
                "url": "https://example.com/@alice/1"
            }
        })
    );
}

#[test]
fn reference_deserializes_id_and_embedded_object_distinctly() {
    let id_reference: Reference<Note> =
        serde_json::from_value(json!("https://example.com/notes/1"))
            .expect("deserialize id reference");
    let object_reference: Reference<Note> = serde_json::from_value(json!({
        "type": "Note",
        "id": "https://example.com/notes/1"
    }))
    .expect("deserialize embedded object reference");

    assert!(matches!(id_reference, Reference::Id(id) if id == iri("https://example.com/notes/1")));
    assert!(
        matches!(object_reference, Reference::Object(note) if note.id == iri("https://example.com/notes/1"))
    );
}

#[test]
fn references_deserialize_single_and_multiple_recipients() {
    let single: References<Iri> =
        serde_json::from_value(json!("https://www.w3.org/ns/activitystreams#Public"))
            .expect("deserialize single recipient");
    let multiple: References<Iri> = serde_json::from_value(json!([
        "https://www.w3.org/ns/activitystreams#Public",
        "https://example.com/users/alice/followers"
    ]))
    .expect("deserialize multiple recipients");

    assert_eq!(
        single,
        References::one(iri("https://www.w3.org/ns/activitystreams#Public"))
    );
    assert_eq!(
        multiple,
        References::many([
            iri("https://www.w3.org/ns/activitystreams#Public"),
            iri("https://example.com/users/alice/followers")
        ])
    );
}

#[test]
fn references_treat_absent_and_empty_array_as_empty() {
    #[derive(Debug, PartialEq, serde::Deserialize, serde::Serialize)]
    struct Recipients {
        #[serde(default, skip_serializing_if = "References::is_empty")]
        to: References<Iri>,
    }

    let absent: Recipients = serde_json::from_value(json!({})).expect("deserialize absent field");
    let empty: Recipients =
        serde_json::from_value(json!({ "to": [] })).expect("deserialize empty field");
    let one = Recipients {
        to: References::one(iri("https://www.w3.org/ns/activitystreams#Public")),
    };

    assert_eq!(absent, empty);
    assert_eq!(
        serde_json::to_value(absent).expect("serialize absent"),
        json!({})
    );
    assert_eq!(
        serde_json::to_value(one).expect("serialize one recipient"),
        json!({ "to": "https://www.w3.org/ns/activitystreams#Public" })
    );
}
