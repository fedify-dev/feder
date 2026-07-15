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
            Input::ReceivedFollow(_) => HandleResult::default(),
            Input::UserCreateNote(input) => {
                let actions = self.state.record_created_note(input);
                HandleResult::new(actions)
            }
        }
    }

    pub fn decide_received_follow(
        &self,
        follow: vocab::Follow,
        state: ReceivedFollowState,
        policy: FollowPolicyDecision,
        context: DecisionContext,
    ) -> Result<Decision, CoreError> {
        if state.already_processed {
            return Ok(Decision::none());
        }

        let Some(following) = reference_id(&follow.object) else {
            return Ok(Decision::none());
        };

        if following != &context.local_actor {
            return Ok(Decision::none());
        }

        let Some(follower) = reference_id(&follow.actor).cloned() else {
            return Ok(Decision::none());
        };

        match policy {
            FollowPolicyDecision::Reject | FollowPolicyDecision::RequireManualApproval => {
                return Ok(Decision::none());
            }
            FollowPolicyDecision::Accept => {}
        }

        let remote_actor = state.remote_actor.ok_or(CoreError::MissingRemoteActor)?;
        let inbox = remote_actor
            .shared_inbox
            .clone()
            .or_else(|| remote_actor.inbox.clone())
            .ok_or(CoreError::MissingInbox)?;

        let accept = vocab::Accept::new(
            context.accept_id,
            vocab::Reference::id(context.local_actor.clone()),
            vocab::Reference::object(follow.clone()),
        );
        let accept = Activity::Accept(accept);

        let mut state_changes = Vec::from([StateChange::RecordProcessedActivity {
            activity_id: follow.id.clone(),
        }]);

        if state.relationship == FollowRelationship::NotFollowing {
            state_changes.push(StateChange::AddFollower {
                local_actor: context.local_actor,
                remote_actor: follower,
                inbox: remote_actor.inbox,
                shared_inbox: remote_actor.shared_inbox,
            });
        }

        state_changes.push(StateChange::StoreActivity {
            activity: accept.clone(),
        });

        Ok(Decision {
            state_changes,
            effects: Vec::from([Effect::PlanDelivery(PlannedDelivery {
                activity: accept,
                inbox,
            })]),
        })
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

/// In-memory state used by portable core flows.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FederState {
    local_actor: vocab::Actor,
    objects: Vec<Object>,
    activities: Vec<Activity>,
}

impl FederState {
    #[must_use]
    pub fn new(config: FederConfig) -> Self {
        Self {
            local_actor: config.local_actor,
            objects: Vec::new(),
            activities: Vec::new(),
        }
    }

    #[must_use]
    pub fn local_actor(&self) -> &vocab::Actor {
        &self.local_actor
    }

    #[must_use]
    pub fn objects(&self) -> &[Object] {
        &self.objects
    }

    #[must_use]
    pub fn activities(&self) -> &[Activity] {
        &self.activities
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

        Vec::from([Action::StoreObject(StoreObject { object })])
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

impl Input {
    pub fn received_follow(follow: vocab::Follow, accept_id: vocab::Iri) -> Self {
        Self::ReceivedFollow(ReceivedFollow { follow, accept_id })
    }
}

/// Runtime-provided stored state for deciding a received Follow.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceivedFollowState {
    pub already_processed: bool,
    pub relationship: FollowRelationship,
    pub remote_actor: Option<RemoteActorState>,
}

/// Current stored relationship between a remote actor and the local actor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FollowRelationship {
    NotFollowing,
    Following,
}

/// Runtime-known state for a remote actor referenced by an input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteActorState {
    pub actor_id: vocab::Iri,
    pub inbox: Option<vocab::Iri>,
    pub shared_inbox: Option<vocab::Iri>,
}

/// Application policy decision for a received Follow.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FollowPolicyDecision {
    Accept,
    Reject,
    RequireManualApproval,
}

/// Deterministic runtime-provided context for one core decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecisionContext {
    pub local_actor: vocab::Iri,
    pub accept_id: vocab::Iri,
}

/// Declarative result of a pure core decision.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Decision {
    pub state_changes: Vec<StateChange>,
    pub effects: Vec<Effect>,
}

impl Decision {
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.state_changes.is_empty() && self.effects.is_empty()
    }
}

/// Durable state change a runtime should apply transactionally.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum StateChange {
    RecordProcessedActivity {
        activity_id: vocab::Iri,
    },
    AddFollower {
        local_actor: vocab::Iri,
        remote_actor: vocab::Iri,
        inbox: Option<vocab::Iri>,
        shared_inbox: Option<vocab::Iri>,
    },
    StoreActivity {
        activity: Activity,
    },
    StoreObject {
        object: Object,
    },
}

/// External work a runtime should plan after durable state is committed.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Effect {
    PlanDelivery(PlannedDelivery),
}

/// Delivery work to persist for a later delivery worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedDelivery {
    pub activity: Activity,
    pub inbox: vocab::Iri,
}

/// Error raised while deciding protocol consequences.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum CoreError {
    MissingRemoteActor,
    MissingInbox,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Follower {
    pub follower: vocab::Iri,
    pub following: vocab::Iri,
}

/// A known actor inbox for future delivery.
///
/// Core records this only when an incoming object embeds enough actor data to
/// expose an inbox. It does not imply every follower has been resolved.
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
    StoreDeliveryTarget(StoreDeliveryTarget),
    StoreObject(StoreObject),
    SendActivity(SendActivity),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoreFollower {
    pub follower: vocab::Reference<vocab::Actor>,
    pub following: vocab::Reference<vocab::Actor>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoreDeliveryTarget {
    pub target: DeliveryTarget,
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
        assert!(core.state().objects().is_empty());
        assert!(core.state().activities().is_empty());
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
}
