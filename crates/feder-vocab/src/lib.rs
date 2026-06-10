//! Minimal Activity Vocabulary types for Feder.
#![no_std]
//!
//! This crate models ActivityPub/ActivityStreams protocol data only. It does
//! not fetch remote objects, read or write storage, deliver activities, or own
//! core decision logic.

extern crate alloc;

use alloc::{boxed::Box, string::String, vec::Vec};
use iri_string::types::IriString;
use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeSeq};

/// The canonical Activity Streams JSON-LD context URL.
pub const ACTIVITYSTREAMS_CONTEXT: &str = "https://www.w3.org/ns/activitystreams";

/// An absolute ActivityPub/ActivityStreams identifier.
pub type Iri = IriString;

/// A non-scalar ActivityStreams property value.
///
/// ActivityStreams object slots can contain either an embedded object or the
/// object's IRI. Phase 1 keeps both forms explicit and avoids dereferencing.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Reference<T> {
    Id(Iri),
    Object(Box<T>),
}

impl<T> Reference<T> {
    #[must_use]
    pub fn id(id: Iri) -> Self {
        Self::Id(id)
    }

    #[must_use]
    pub fn object(object: T) -> Self {
        Self::Object(Box::new(object))
    }
}

/// Zero or more ActivityStreams property values.
///
/// Use this with `#[serde(default, skip_serializing_if = "References::is_empty")]`
/// on containing fields. Empty values then serialize as absent, one value
/// serializes as a scalar, and multiple values serialize as an array.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct References<T> {
    values: Vec<T>,
}

impl<T> Default for References<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> References<T> {
    #[must_use]
    pub fn new() -> Self {
        Self { values: Vec::new() }
    }

    #[must_use]
    pub fn one(value: T) -> Self {
        Self {
            values: Vec::from([value]),
        }
    }

    #[must_use]
    pub fn many(values: impl Into<Vec<T>>) -> Self {
        Self {
            values: values.into(),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn iter(&self) -> core::slice::Iter<'_, T> {
        self.values.iter()
    }

    pub fn into_vec(self) -> Vec<T> {
        self.values
    }
}

impl<T> From<Vec<T>> for References<T> {
    fn from(values: Vec<T>) -> Self {
        Self::many(values)
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OneOrMany<T> {
    One(T),
    Many(Vec<T>),
}

impl<T> Serialize for References<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.values.as_slice() {
            [] => {
                let sequence = serializer.serialize_seq(Some(0))?;
                sequence.end()
            }
            [value] => value.serialize(serializer),
            values => values.serialize(serializer),
        }
    }
}

impl<'de, T> Deserialize<'de> for References<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match OneOrMany::deserialize(deserializer)? {
            OneOrMany::One(value) => Ok(References::one(value)),
            OneOrMany::Many(values) => Ok(References::many(values)),
        }
    }
}

macro_rules! activitystreams_type {
    ($name:ident, $variant:ident) => {
        #[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
        pub enum $name {
            #[default]
            $variant,
        }
    };
}

activitystreams_type!(NoteType, Note);
activitystreams_type!(FollowType, Follow);
activitystreams_type!(AcceptType, Accept);
activitystreams_type!(CreateType, Create);

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum ActorType {
    Application,
    Group,
    Organization,
    #[default]
    Person,
    Service,
}

/// A minimal ActivityPub actor for Phase 1 core tests.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Actor {
    #[serde(rename = "@context", skip_serializing_if = "Option::is_none")]
    pub context: Option<Iri>,
    #[serde(rename = "type")]
    pub kind: ActorType,
    pub id: Iri,
    pub inbox: Iri,
    pub outbox: Iri,
    #[serde(rename = "preferredUsername", skip_serializing_if = "Option::is_none")]
    pub preferred_username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Actor {
    #[must_use]
    pub fn person(id: Iri, inbox: Iri, outbox: Iri) -> Self {
        Self::new(ActorType::Person, id, inbox, outbox)
    }

    #[must_use]
    pub fn new(kind: ActorType, id: Iri, inbox: Iri, outbox: Iri) -> Self {
        Self {
            context: Some(
                ACTIVITYSTREAMS_CONTEXT
                    .parse()
                    .expect("valid ActivityStreams IRI"),
            ),
            kind,
            id,
            inbox,
            outbox,
            preferred_username: None,
            name: None,
        }
    }
}

/// A minimal ActivityStreams Note object.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Note {
    #[serde(rename = "@context", skip_serializing_if = "Option::is_none")]
    pub context: Option<Iri>,
    #[serde(rename = "type")]
    pub kind: NoteType,
    pub id: Iri,
    #[serde(rename = "attributedTo", skip_serializing_if = "Option::is_none")]
    pub attributed_to: Option<Reference<Actor>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published: Option<String>,
}

impl Note {
    #[must_use]
    pub fn new(id: Iri) -> Self {
        Self {
            context: Some(
                ACTIVITYSTREAMS_CONTEXT
                    .parse()
                    .expect("valid ActivityStreams IRI"),
            ),
            kind: NoteType::default(),
            id,
            attributed_to: None,
            content: None,
            published: None,
        }
    }
}

/// A minimal Follow activity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Follow {
    #[serde(rename = "@context", skip_serializing_if = "Option::is_none")]
    pub context: Option<Iri>,
    #[serde(rename = "type")]
    pub kind: FollowType,
    pub id: Iri,
    pub actor: Reference<Actor>,
    pub object: Reference<Actor>,
}

impl Follow {
    #[must_use]
    pub fn new(id: Iri, actor: Reference<Actor>, object: Reference<Actor>) -> Self {
        Self {
            context: Some(
                ACTIVITYSTREAMS_CONTEXT
                    .parse()
                    .expect("valid ActivityStreams IRI"),
            ),
            kind: FollowType::default(),
            id,
            actor,
            object,
        }
    }
}

/// A minimal Accept activity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Accept {
    #[serde(rename = "@context", skip_serializing_if = "Option::is_none")]
    pub context: Option<Iri>,
    #[serde(rename = "type")]
    pub kind: AcceptType,
    pub id: Iri,
    pub actor: Reference<Actor>,
    pub object: Reference<Follow>,
}

impl Accept {
    #[must_use]
    pub fn new(id: Iri, actor: Reference<Actor>, object: Reference<Follow>) -> Self {
        Self {
            context: Some(
                ACTIVITYSTREAMS_CONTEXT
                    .parse()
                    .expect("valid ActivityStreams IRI"),
            ),
            kind: AcceptType::default(),
            id,
            actor,
            object,
        }
    }
}

/// A minimal Create activity for a concrete object type.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Create<T> {
    #[serde(rename = "@context", skip_serializing_if = "Option::is_none")]
    pub context: Option<Iri>,
    #[serde(rename = "type")]
    pub kind: CreateType,
    pub id: Iri,
    pub actor: Reference<Actor>,
    pub object: Reference<T>,
}

impl<T> Create<T> {
    #[must_use]
    pub fn new(id: Iri, actor: Reference<Actor>, object: Reference<T>) -> Self {
        Self {
            context: Some(
                ACTIVITYSTREAMS_CONTEXT
                    .parse()
                    .expect("valid ActivityStreams IRI"),
            ),
            kind: CreateType::default(),
            id,
            actor,
            object,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use serde::de::DeserializeOwned;
    use serde_json::json;

    fn roundtrip<T>(value: &T) -> T
    where
        T: DeserializeOwned + Serialize,
    {
        let json = serde_json::to_string(value).expect("serialize activitystreams value");
        serde_json::from_str(&json).expect("deserialize activitystreams value")
    }

    fn iri(value: &str) -> Iri {
        value.parse().expect("valid test IRI")
    }

    #[test]
    fn actor_roundtrips_json() {
        let mut actor = Actor::person(
            iri("https://example.com/users/alice"),
            iri("https://example.com/users/alice/inbox"),
            iri("https://example.com/users/alice/outbox"),
        );
        actor.preferred_username = Some("alice".to_string());
        actor.name = Some("Alice".to_string());

        assert_eq!(roundtrip(&actor), actor);
    }

    #[test]
    fn actor_deserializes_basic_activitypub_json() {
        let actor: Actor = serde_json::from_value(json!({
            "@context": ACTIVITYSTREAMS_CONTEXT,
            "type": "Person",
            "id": "https://example.com/users/alice",
            "inbox": "https://example.com/users/alice/inbox",
            "outbox": "https://example.com/users/alice/outbox",
            "preferredUsername": "alice",
            "name": "Alice"
        }))
        .expect("deserialize actor from json");

        assert_eq!(actor.id, iri("https://example.com/users/alice"));
        assert_eq!(actor.preferred_username, Some("alice".to_string()));
    }

    #[test]
    fn actor_deserializes_non_person_activitypub_json() {
        let actor: Actor = serde_json::from_value(json!({
            "@context": ACTIVITYSTREAMS_CONTEXT,
            "type": "Service",
            "id": "https://example.com/actors/service",
            "inbox": "https://example.com/actors/service/inbox",
            "outbox": "https://example.com/actors/service/outbox",
            "name": "Feder Service"
        }))
        .expect("deserialize service actor from json");

        assert_eq!(actor.kind, ActorType::Service);
        assert_eq!(actor.id, iri("https://example.com/actors/service"));
    }

    #[test]
    fn follow_and_accept_roundtrip_json() {
        let follow = Follow::new(
            iri("https://remote.example/activities/follow/1"),
            Reference::id(iri("https://remote.example/users/bob")),
            Reference::id(iri("https://example.com/users/alice")),
        );
        let accept = Accept::new(
            iri("https://example.com/activities/accept/1"),
            Reference::id(iri("https://example.com/users/alice")),
            Reference::object(follow),
        );

        assert_eq!(roundtrip(&accept), accept);
    }

    #[test]
    fn create_note_roundtrips_json() {
        let mut note = Note::new(iri("https://example.com/notes/1"));
        note.attributed_to = Some(Reference::id(iri("https://example.com/users/alice")));
        note.content = Some("Hello, fediverse.".to_string());
        note.published = Some("2026-05-29T06:30:00Z".to_string());

        let create = Create::new(
            iri("https://example.com/activities/create/1"),
            Reference::id(iri("https://example.com/users/alice")),
            Reference::object(note),
        );

        assert_eq!(roundtrip(&create), create);
    }

    #[test]
    fn concrete_types_reject_wrong_activitystreams_type() {
        let result = serde_json::from_value::<Follow>(json!({
            "type": "Accept",
            "id": "https://remote.example/activities/follow/1",
            "actor": "https://remote.example/users/bob",
            "object": "https://example.com/users/alice"
        }));

        assert!(result.is_err());
    }

    #[test]
    fn references_deserializes_scalar_and_array() {
        let one: References<Iri> = serde_json::from_value(json!("https://example.com/users/alice"))
            .expect("deserialize scalar references value");
        let many: References<Iri> = serde_json::from_value(json!([
            "https://example.com/users/alice",
            "https://example.com/users/bob"
        ]))
        .expect("deserialize array references value");

        assert_eq!(one, References::one(iri("https://example.com/users/alice")));
        assert_eq!(
            many,
            References::many([
                iri("https://example.com/users/alice"),
                iri("https://example.com/users/bob")
            ])
        );
    }

    #[test]
    fn references_serializes_empty_one_and_many() {
        assert_eq!(
            serde_json::to_value(References::<Iri>::new()).expect("serialize empty references"),
            json!([])
        );
        assert_eq!(
            serde_json::to_value(References::one(iri("https://example.com/users/alice")))
                .expect("serialize one reference"),
            json!("https://example.com/users/alice")
        );
        assert_eq!(
            serde_json::to_value(References::many([
                iri("https://example.com/users/alice"),
                iri("https://example.com/users/bob")
            ]))
            .expect("serialize many references"),
            json!([
                "https://example.com/users/alice",
                "https://example.com/users/bob"
            ])
        );
    }
}
