//! Portable ActivityPub core logic for Feder.
#![no_std]

extern crate alloc;

use alloc::{string::String, vec::Vec};

pub use feder_vocab as vocab;

/// Portable core state and decision logic.
#[derive(Debug)]
pub struct FederCore {
    state: FederState,
}

impl FederCore {
    #[must_use]
    pub fn new(config: FederConfig) -> Self {
        Self {
            state: FederState::new(config),
        }
    }

    #[must_use]
    pub fn state(&self) -> &FederState {
        &self.state
    }

    /// Handle one core input and return runtime actions to perform later.
    ///
    /// This method intentionally performs no I/O. Follow acceptance and delivery
    /// behavior are added by later Phase 1 issues.
    #[must_use]
    pub fn handle(&mut self, input: Input) -> HandleResult {
        match input {
            Input::ReceivedFollow(follow) => {
                self.state.record_follow(follow);
                HandleResult::default()
            }
            Input::UserCreateNote(input) => {
                self.state.record_created_note(input);
                HandleResult::default()
            }
        }
    }
}

/// Runtime-provided configuration for portable core state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FederConfig {
    pub local_actor: vocab::Actor,
}

impl FederConfig {
    #[must_use]
    pub fn new(local_actor: vocab::Actor) -> Self {
        Self { local_actor }
    }
}

/// In-memory state used by Phase 1 core flows.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FederState {
    local_actor: vocab::Actor,
    followers: Vec<Follower>,
    delivery_targets: Vec<DeliveryTarget>,
    objects: Vec<Object>,
    activities: Vec<Activity>,
}

impl FederState {
    #[must_use]
    pub fn new(config: FederConfig) -> Self {
        Self {
            local_actor: config.local_actor,
            followers: Vec::new(),
            delivery_targets: Vec::new(),
            objects: Vec::new(),
            activities: Vec::new(),
        }
    }

    #[must_use]
    pub fn local_actor(&self) -> &vocab::Actor {
        &self.local_actor
    }

    #[must_use]
    pub fn followers(&self) -> &[Follower] {
        &self.followers
    }

    #[must_use]
    /// Delivery targets known from embedded actor data.
    ///
    /// ID-only followers are tracked in `followers`, but they do not produce a
    /// delivery target until a runtime or later core flow resolves actor data.
    pub fn delivery_targets(&self) -> &[DeliveryTarget] {
        &self.delivery_targets
    }

    #[must_use]
    pub fn objects(&self) -> &[Object] {
        &self.objects
    }

    #[must_use]
    pub fn activities(&self) -> &[Activity] {
        &self.activities
    }

    fn record_follow(&mut self, follow: vocab::Follow) {
        let Some(following) = reference_id(&follow.object).cloned() else {
            return;
        };

        if following != self.local_actor.id {
            return;
        }

        let Some(follower) = reference_id(&follow.actor).cloned() else {
            return;
        };

        let relation = Follower {
            follower: follower.clone(),
            following,
        };

        if !self.followers.contains(&relation) {
            self.followers.push(relation);
        }

        if let vocab::Reference::Object(actor) = follow.actor {
            let target = DeliveryTarget {
                actor: follower,
                inbox: actor.inbox,
            };

            if let Some(existing) = self
                .delivery_targets
                .iter_mut()
                .find(|existing| existing.actor == target.actor)
            {
                existing.inbox = target.inbox;
            } else {
                self.delivery_targets.push(target);
            }
        }
    }

    fn record_created_note(&mut self, input: UserCreateNote) {
        let Some(actor) = reference_id(&input.actor) else {
            return;
        };

        if actor != &self.local_actor.id {
            return;
        }

        let actor = vocab::Reference::id(self.local_actor.id.clone());

        let mut note = vocab::Note::new(input.note_id);
        note.attributed_to = Some(actor.clone());
        note.content = Some(input.content);
        note.published = input.published;

        let create = vocab::Create::new(
            input.create_id,
            actor,
            vocab::Reference::object(note.clone()),
        );

        self.objects.push(Object::Note(note));
        self.activities.push(Activity::CreateNote(create));
    }
}

fn reference_id<T>(reference: &vocab::Reference<T>) -> Option<&vocab::Iri>
where
    T: HasId,
{
    match reference {
        vocab::Reference::Id(id) => Some(id),
        vocab::Reference::Object(object) => Some(object.id()),
    }
}

trait HasId {
    fn id(&self) -> &vocab::Iri;
}

impl HasId for vocab::Actor {
    fn id(&self) -> &vocab::Iri {
        &self.id
    }
}

/// Something entering the portable core from a runtime.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Follower {
    pub follower: vocab::Iri,
    pub following: vocab::Iri,
}

/// A known actor inbox for future delivery.
///
/// Phase 1 records this only when an incoming object embeds enough actor data
/// to expose an inbox. It does not imply every follower has been resolved.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliveryTarget {
    pub actor: vocab::Iri,
    pub inbox: vocab::Iri,
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

    fn core() -> FederCore {
        FederCore::new(FederConfig::new(actor("https://example.com/users/alice")))
    }

    #[test]
    fn core_is_created_with_local_actor_state() {
        let core = core();

        assert_eq!(
            core.state().local_actor().id,
            iri("https://example.com/users/alice")
        );
        assert!(core.state().followers().is_empty());
        assert!(core.state().delivery_targets().is_empty());
        assert!(core.state().objects().is_empty());
        assert!(core.state().activities().is_empty());
    }

    #[test]
    fn received_follow_updates_followers_and_delivery_targets() {
        let mut core = core();
        let follow = vocab::Follow::new(
            iri("https://remote.example/activities/follow/1"),
            vocab::Reference::object(actor("https://remote.example/users/bob")),
            vocab::Reference::id(iri("https://example.com/users/alice")),
        );

        let result = core.handle(Input::ReceivedFollow(follow));

        assert!(result.is_empty());
        assert_eq!(
            core.state().followers(),
            &[Follower {
                follower: iri("https://remote.example/users/bob"),
                following: iri("https://example.com/users/alice"),
            }]
        );
        assert_eq!(
            core.state().delivery_targets(),
            &[DeliveryTarget {
                actor: iri("https://remote.example/users/bob"),
                inbox: iri("https://remote.example/users/bob/inbox"),
            }]
        );
    }

    #[test]
    fn received_follow_updates_existing_delivery_target_by_actor() {
        let mut core = core();
        let first_follow = vocab::Follow::new(
            iri("https://remote.example/activities/follow/1"),
            vocab::Reference::object(actor("https://remote.example/users/bob")),
            vocab::Reference::id(iri("https://example.com/users/alice")),
        );

        let mut updated_actor = actor("https://remote.example/users/bob");
        updated_actor.inbox = iri("https://remote.example/inboxes/bob");
        let second_follow = vocab::Follow::new(
            iri("https://remote.example/activities/follow/2"),
            vocab::Reference::object(updated_actor),
            vocab::Reference::id(iri("https://example.com/users/alice")),
        );

        assert!(core.handle(Input::ReceivedFollow(first_follow)).is_empty());
        assert!(core.handle(Input::ReceivedFollow(second_follow)).is_empty());

        assert_eq!(
            core.state().followers(),
            &[Follower {
                follower: iri("https://remote.example/users/bob"),
                following: iri("https://example.com/users/alice"),
            }]
        );
        assert_eq!(
            core.state().delivery_targets(),
            &[DeliveryTarget {
                actor: iri("https://remote.example/users/bob"),
                inbox: iri("https://remote.example/inboxes/bob"),
            }]
        );
    }

    #[test]
    fn received_follow_with_actor_id_records_follower_without_delivery_target() {
        let mut core = core();
        let follow = vocab::Follow::new(
            iri("https://remote.example/activities/follow/1"),
            vocab::Reference::id(iri("https://remote.example/users/bob")),
            vocab::Reference::id(iri("https://example.com/users/alice")),
        );

        let result = core.handle(Input::ReceivedFollow(follow));

        assert!(result.is_empty());
        assert_eq!(
            core.state().followers(),
            &[Follower {
                follower: iri("https://remote.example/users/bob"),
                following: iri("https://example.com/users/alice"),
            }]
        );
        assert!(core.state().delivery_targets().is_empty());
    }

    #[test]
    fn received_follow_for_other_actor_is_ignored() {
        let mut core = core();
        let follow = vocab::Follow::new(
            iri("https://remote.example/activities/follow/1"),
            vocab::Reference::object(actor("https://remote.example/users/bob")),
            vocab::Reference::id(iri("https://example.com/users/other")),
        );

        let result = core.handle(Input::ReceivedFollow(follow));

        assert!(result.is_empty());
        assert!(core.state().followers().is_empty());
        assert!(core.state().delivery_targets().is_empty());
    }

    #[test]
    fn user_create_note_records_created_object_and_activity() {
        let input = UserCreateNote {
            note_id: iri("https://example.com/notes/1"),
            create_id: iri("https://example.com/activities/create/1"),
            actor: vocab::Reference::id(iri("https://example.com/users/alice")),
            content: "Hello from Feder.".to_string(),
            published: Some("2026-06-10T00:00:00Z".to_string()),
        };

        let mut core = core();
        let result = core.handle(Input::UserCreateNote(input));

        assert!(result.is_empty());
        assert_eq!(core.state().objects().len(), 1);
        assert_eq!(core.state().activities().len(), 1);

        let Object::Note(note) = &core.state().objects()[0];
        assert_eq!(note.id, iri("https://example.com/notes/1"));
        assert_eq!(
            note.attributed_to,
            Some(vocab::Reference::id(iri("https://example.com/users/alice")))
        );
        assert_eq!(note.content, Some("Hello from Feder.".to_string()));
        assert_eq!(note.published, Some("2026-06-10T00:00:00Z".to_string()));

        match &core.state().activities()[0] {
            Activity::CreateNote(create) => {
                assert_eq!(create.id, iri("https://example.com/activities/create/1"));
                assert_eq!(
                    create.actor,
                    vocab::Reference::id(iri("https://example.com/users/alice"))
                );
            }
            Activity::Accept(_) => panic!("expected Create<Note> activity"),
        }
    }

    #[test]
    fn user_create_note_normalizes_embedded_local_actor_to_local_actor_id() {
        let mut supplied_actor = actor("https://example.com/users/alice");
        supplied_actor.inbox = iri("https://untrusted.example/inbox");

        let input = UserCreateNote {
            note_id: iri("https://example.com/notes/1"),
            create_id: iri("https://example.com/activities/create/1"),
            actor: vocab::Reference::object(supplied_actor),
            content: "Hello from Feder.".to_string(),
            published: None,
        };

        let mut core = core();
        let result = core.handle(Input::UserCreateNote(input));

        assert!(result.is_empty());

        let Object::Note(note) = &core.state().objects()[0];
        assert_eq!(
            note.attributed_to,
            Some(vocab::Reference::id(iri("https://example.com/users/alice")))
        );

        let Activity::CreateNote(create) = &core.state().activities()[0] else {
            panic!("expected Create<Note> activity");
        };
        assert_eq!(
            create.actor,
            vocab::Reference::id(iri("https://example.com/users/alice"))
        );
    }

    #[test]
    fn user_create_note_for_non_local_actor_is_ignored() {
        let input = UserCreateNote {
            note_id: iri("https://remote.example/notes/1"),
            create_id: iri("https://remote.example/activities/create/1"),
            actor: vocab::Reference::id(iri("https://remote.example/users/bob")),
            content: "Hello from elsewhere.".to_string(),
            published: Some("2026-06-10T00:00:00Z".to_string()),
        };

        let mut core = core();
        let result = core.handle(Input::UserCreateNote(input));

        assert!(result.is_empty());
        assert!(core.state().objects().is_empty());
        assert!(core.state().activities().is_empty());
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
