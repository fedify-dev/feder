//! Portable ActivityPub core logic for Feder.
#![no_std]

extern crate alloc;

use alloc::{string::String, vec::Vec};

pub use feder_vocab as vocab;

/// Portable core state and decision logic.
#[derive(Debug, Default)]
pub struct FederCore;

impl FederCore {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Handle one core input and return runtime actions to perform later.
    ///
    /// This method intentionally performs no I/O. Follow acceptance, object
    /// storage, and delivery behavior are added by later Phase 1 issues.
    #[must_use]
    pub fn handle(&mut self, input: Input) -> HandleResult {
        match input {
            Input::ReceivedFollow(_) | Input::UserCreateNote(_) => HandleResult::default(),
        }
    }
}

/// Something entering the portable core from a runtime.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Input {
    ReceivedFollow(vocab::Follow),
    UserCreateNote(UserCreateNote),
}

/// Runtime-provided data for creating a local note.
///
/// IDs and timestamps are inputs so the core does not depend on clocks,
/// randomness, or platform-specific ID generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserCreateNote {
    pub note_id: vocab::Iri,
    pub create_id: vocab::Iri,
    pub actor: vocab::Reference<vocab::Actor>,
    pub content: String,
    pub published: Option<String>,
}

/// Something the runtime should perform after core handling.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Action {
    StoreFollower(StoreFollower),
    StoreObject(StoreObject),
    SendActivity(SendActivity),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoreFollower {
    pub follower: vocab::Reference<vocab::Actor>,
    pub following: vocab::Reference<vocab::Actor>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoreObject {
    pub object: Object,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SendActivity {
    pub activity: Activity,
    pub inbox: vocab::Iri,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Activity {
    Accept(vocab::Accept),
    CreateNote(vocab::Create<vocab::Note>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Object {
    Note(vocab::Note),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HandleResult {
    pub actions: Vec<Action>,
}

impl HandleResult {
    #[must_use]
    pub fn new(actions: Vec<Action>) -> Self {
        Self { actions }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;
    use alloc::string::ToString;

    fn iri(value: &str) -> vocab::Iri {
        value.parse().expect("valid test IRI")
    }

    fn actor(id: &str) -> vocab::Actor {
        vocab::Actor::person(
            iri(id),
            iri(&format!("{id}/inbox")),
            iri(&format!("{id}/outbox")),
        )
    }

    #[test]
    fn received_follow_enters_core_without_runtime_io() {
        let mut core = FederCore::new();
        let follow = vocab::Follow::new(
            iri("https://remote.example/activities/follow/1"),
            vocab::Reference::id(iri("https://remote.example/users/bob")),
            vocab::Reference::object(actor("https://example.com/users/alice")),
        );

        let result = core.handle(Input::ReceivedFollow(follow));

        assert!(result.is_empty());
    }

    #[test]
    fn user_create_note_input_carries_nondeterministic_values() {
        let input = UserCreateNote {
            note_id: iri("https://example.com/notes/1"),
            create_id: iri("https://example.com/activities/create/1"),
            actor: vocab::Reference::id(iri("https://example.com/users/alice")),
            content: "Hello from Feder.".to_string(),
            published: Some("2026-06-10T00:00:00Z".to_string()),
        };

        let mut core = FederCore::new();
        let result = core.handle(Input::UserCreateNote(input));

        assert!(result.is_empty());
    }

    #[test]
    fn handle_result_wraps_action_lists() {
        let result = HandleResult::new(Vec::from([Action::StoreFollower(StoreFollower {
            follower: vocab::Reference::id(iri("https://remote.example/users/bob")),
            following: vocab::Reference::id(iri("https://example.com/users/alice")),
        })]));

        assert_eq!(result.actions.len(), 1);
    }
}
