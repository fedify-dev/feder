use feder_vocab::{
    ACTIVITYSTREAMS_CONTEXT, Accept, Create, Follow, Iri, Note, Reference, References,
};
use serde_json::{Value, json};

fn serialize(value: impl serde::Serialize) -> Value {
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
        serialize(outgoing_accept),
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
fn local_note_can_shape_create_note_activity() {
    let mut note = Note::new(iri("https://example.com/notes/1"));
    note.attributed_to = Some(Reference::id(iri("https://example.com/users/alice")));
    note.content = Some("Hello from Feder.".to_string());
    note.published = Some("2026-06-02T00:00:00Z".to_string());

    let create = Create::new(
        iri("https://example.com/activities/create/1"),
        Reference::id(iri("https://example.com/users/alice")),
        Reference::object(note),
    );

    assert_eq!(
        serialize(create),
        json!({
            "@context": ACTIVITYSTREAMS_CONTEXT,
            "type": "Create",
            "id": "https://example.com/activities/create/1",
            "actor": "https://example.com/users/alice",
            "object": {
                "@context": ACTIVITYSTREAMS_CONTEXT,
                "type": "Note",
                "id": "https://example.com/notes/1",
                "attributedTo": "https://example.com/users/alice",
                "content": "Hello from Feder.",
                "published": "2026-06-02T00:00:00Z"
            }
        })
    );
}

#[test]
fn reference_keeps_id_and_embedded_object_shapes_distinct() {
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
fn references_can_represent_common_recipient_shapes() {
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
