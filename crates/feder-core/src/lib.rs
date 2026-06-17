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
    /// This method intentionally performs no I/O. Returned actions describe
    /// work for a runtime or test harness to perform later.
    #[must_use]
    pub fn handle(&mut self, input: Input) -> HandleResult {
        match input {
            Input::ReceivedFollow(input) => {
                let actions = self.state.record_follow(input);
                HandleResult::new(actions)
            }
            Input::UserCreateNote(input) => {
                let actions = self.state.record_created_note(input);
                HandleResult::new(actions)
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

    fn record_follow(&mut self, input: ReceivedFollow) -> Vec<Action> {
        let follow = input.follow;
        let Some(following) = reference_id(&follow.object).cloned() else {
            return Vec::new();
        };

        if following != self.local_actor.id {
            return Vec::new();
        }

        let Some(follower) = reference_id(&follow.actor).cloned() else {
            return Vec::new();
        };

        let relation = Follower {
            follower: follower.clone(),
            following,
        };
        let mut actions = Vec::new();

        if !self.followers.contains(&relation) {
            self.followers.push(relation.clone());
        }

        actions.push(Action::StoreFollower(StoreFollower {
            follower: follow.actor.clone(),
            following: follow.object.clone(),
        }));

        let mut inbox = self
            .delivery_targets
            .iter()
            .find(|target| target.actor == follower)
            .map(|target| target.inbox.clone());

        if let vocab::Reference::Object(actor) = &follow.actor {
            let target = DeliveryTarget {
                actor: follower,
                inbox: actor.inbox.clone(),
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

            inbox = Some(actor.inbox.clone());
        }

        if let Some(inbox) = inbox {
            let accept = vocab::Accept::new(
                input.accept_id,
                vocab::Reference::id(self.local_actor.id.clone()),
                vocab::Reference::object(follow),
            );

            actions.push(Action::SendActivity(SendActivity {
                activity: Activity::Accept(accept),
                inbox,
            }));
        }

        actions
    }

    fn record_created_note(&mut self, input: UserCreateNote) -> Vec<Action> {
        let Some(actor) = reference_id(&input.actor) else {
            return Vec::new();
        };

        if actor != &self.local_actor.id {
            return Vec::new();
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

        let object = Object::Note(note);
        self.objects.push(object.clone());
        self.activities.push(Activity::CreateNote(create.clone()));

        let mut actions = Vec::from([Action::StoreObject(StoreObject { object })]);

        actions.extend(self.delivery_targets.iter().map(|target| {
            Action::SendActivity(SendActivity {
                activity: Activity::CreateNote(create.clone()),
                inbox: target.inbox.clone(),
            })
        }));

        actions
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
    ReceivedFollow(ReceivedFollow),
    UserCreateNote(UserCreateNote),
}

/// Runtime-provided data for handling a received Follow.
///
/// The Accept activity ID is an input so the core does not depend on clocks,
/// randomness, or platform-specific ID generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceivedFollow {
    pub follow: vocab::Follow,
    pub accept_id: vocab::Iri,
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

    fn received_follow(follow: vocab::Follow, id: &str) -> Input {
        Input::ReceivedFollow(ReceivedFollow {
            follow,
            accept_id: iri(id),
        })
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
    fn received_follow_records_follower_and_emits_accept_actions() {
        let mut core = core();
        let follow = vocab::Follow::new(
            iri("https://remote.example/activities/follow/1"),
            vocab::Reference::object(actor("https://remote.example/users/bob")),
            vocab::Reference::id(iri("https://example.com/users/alice")),
        );

        let result = core.handle(received_follow(
            follow,
            "https://example.com/activities/accept/1",
        ));

        assert_eq!(result.actions.len(), 2);
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
        assert_eq!(
            result.actions[0],
            Action::StoreFollower(StoreFollower {
                follower: vocab::Reference::object(actor("https://remote.example/users/bob")),
                following: vocab::Reference::id(iri("https://example.com/users/alice")),
            })
        );

        let Action::SendActivity(send) = &result.actions[1] else {
            panic!("expected SendActivity action");
        };
        assert_eq!(send.inbox, iri("https://remote.example/users/bob/inbox"));

        let Activity::Accept(accept) = &send.activity else {
            panic!("expected Accept activity");
        };
        assert_eq!(accept.id, iri("https://example.com/activities/accept/1"));
        assert_eq!(
            accept.actor,
            vocab::Reference::id(iri("https://example.com/users/alice"))
        );
        let vocab::Reference::Object(accepted_follow) = &accept.object else {
            panic!("expected embedded Follow object");
        };
        assert_eq!(
            accepted_follow.id,
            iri("https://remote.example/activities/follow/1")
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

        let first_result = core.handle(received_follow(
            first_follow,
            "https://example.com/activities/accept/1",
        ));
        let second_result = core.handle(received_follow(
            second_follow,
            "https://example.com/activities/accept/2",
        ));

        assert_eq!(first_result.actions.len(), 2);
        assert_eq!(second_result.actions.len(), 2);

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

        let result = core.handle(received_follow(
            follow,
            "https://example.com/activities/accept/1",
        ));

        assert_eq!(
            result.actions,
            Vec::from([Action::StoreFollower(StoreFollower {
                follower: vocab::Reference::id(iri("https://remote.example/users/bob")),
                following: vocab::Reference::id(iri("https://example.com/users/alice")),
            })])
        );
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

        let result = core.handle(received_follow(
            follow,
            "https://example.com/activities/accept/1",
        ));

        assert!(result.is_empty());
        assert!(core.state().followers().is_empty());
        assert!(core.state().delivery_targets().is_empty());
    }

    #[test]
    fn user_create_note_records_created_object_and_emits_store_action() {
        let input = UserCreateNote {
            note_id: iri("https://example.com/notes/1"),
            create_id: iri("https://example.com/activities/create/1"),
            actor: vocab::Reference::id(iri("https://example.com/users/alice")),
            content: "Hello from Feder.".to_string(),
            published: Some("2026-06-10T00:00:00Z".to_string()),
        };

        let mut core = core();
        let result = core.handle(Input::UserCreateNote(input));

        assert_eq!(result.actions.len(), 1);
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

        assert_eq!(
            result.actions[0],
            Action::StoreObject(StoreObject {
                object: Object::Note(note.clone()),
            })
        );
    }

    #[test]
    fn user_create_note_emits_create_activity_for_known_delivery_targets() {
        let mut core = core();
        let follow = vocab::Follow::new(
            iri("https://remote.example/activities/follow/1"),
            vocab::Reference::object(actor("https://remote.example/users/bob")),
            vocab::Reference::id(iri("https://example.com/users/alice")),
        );
        let _ = core.handle(received_follow(
            follow,
            "https://example.com/activities/accept/1",
        ));

        let input = UserCreateNote {
            note_id: iri("https://example.com/notes/1"),
            create_id: iri("https://example.com/activities/create/1"),
            actor: vocab::Reference::id(iri("https://example.com/users/alice")),
            content: "Hello from Feder.".to_string(),
            published: Some("2026-06-10T00:00:00Z".to_string()),
        };

        let result = core.handle(Input::UserCreateNote(input));

        assert_eq!(result.actions.len(), 2);
        let Action::StoreObject(store) = &result.actions[0] else {
            panic!("expected StoreObject action");
        };
        let Object::Note(note) = &store.object;
        assert_eq!(note.id, iri("https://example.com/notes/1"));

        let Action::SendActivity(send) = &result.actions[1] else {
            panic!("expected SendActivity action");
        };
        assert_eq!(send.inbox, iri("https://remote.example/users/bob/inbox"));

        let Activity::CreateNote(create) = &send.activity else {
            panic!("expected Create<Note> activity");
        };
        assert_eq!(create.id, iri("https://example.com/activities/create/1"));
        assert_eq!(
            create.actor,
            vocab::Reference::id(iri("https://example.com/users/alice"))
        );
        let vocab::Reference::Object(created_note) = &create.object else {
            panic!("expected embedded Note object");
        };
        assert_eq!(created_note.id, iri("https://example.com/notes/1"));
    }

    #[test]
    fn user_create_note_emits_create_activity_for_each_known_delivery_target() {
        let mut core = core();
        for (index, follower) in [
            "https://remote.example/users/bob",
            "https://another.example/users/carol",
        ]
        .into_iter()
        .enumerate()
        {
            let follow = vocab::Follow::new(
                iri(&format!("https://example.com/activities/follow/{index}")),
                vocab::Reference::object(actor(follower)),
                vocab::Reference::id(iri("https://example.com/users/alice")),
            );
            let _ = core.handle(received_follow(
                follow,
                &format!("https://example.com/activities/accept/{index}"),
            ));
        }

        let input = UserCreateNote {
            note_id: iri("https://example.com/notes/1"),
            create_id: iri("https://example.com/activities/create/1"),
            actor: vocab::Reference::id(iri("https://example.com/users/alice")),
            content: "Hello from Feder.".to_string(),
            published: None,
        };

        let result = core.handle(Input::UserCreateNote(input));

        assert_eq!(result.actions.len(), 3);
        assert!(matches!(result.actions[0], Action::StoreObject(_)));

        let expected_inboxes = [
            iri("https://remote.example/users/bob/inbox"),
            iri("https://another.example/users/carol/inbox"),
        ];

        for (action, expected_inbox) in result.actions[1..].iter().zip(expected_inboxes) {
            let Action::SendActivity(send) = action else {
                panic!("expected SendActivity action");
            };
            assert_eq!(send.inbox, expected_inbox);
            assert!(matches!(send.activity, Activity::CreateNote(_)));
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

        assert_eq!(result.actions.len(), 1);

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
