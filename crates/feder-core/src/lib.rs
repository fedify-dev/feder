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

#[cfg(feature = "http-signatures")]
pub mod http_signatures;

const PUBLIC_COLLECTION: &str = "https://www.w3.org/ns/activitystreams#Public";

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
            Input::ReceivedUndoFollow(input) => {
                let actions = self.state.record_undo_follow(input);
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

/// In-memory state used by portable core flows.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FederState {
    local_actor: vocab::Actor,
    followers: Vec<Follower>,
    objects: Vec<Object>,
    activities: Vec<Activity>,
}

impl FederState {
    #[must_use]
    pub fn new(config: FederConfig) -> Self {
        Self {
            local_actor: config.local_actor,
            followers: Vec::new(),
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
    pub fn objects(&self) -> &[Object] {
        &self.objects
    }

    #[must_use]
    pub fn activities(&self) -> &[Activity] {
        &self.activities
    }

    fn record_follow(&mut self, input: ReceivedFollow) -> Vec<Action> {
        let follow = input.follow;
        let Some(following) = reference_id(&follow.object) else {
            return Vec::new();
        };

        if following != &self.local_actor.id {
            return Vec::new();
        }

        let Some(follower) = reference_id(&follow.actor).cloned() else {
            return Vec::new();
        };

        let relation = Follower {
            follower: follower.clone(),
            following: following.clone(),
        };
        let mut actions = Vec::new();

        if !self.followers.contains(&relation) {
            self.followers.push(relation.clone());
        }

        actions.push(Action::StoreFollower(StoreFollower {
            follower: follow.actor.clone(),
            following: follow.object.clone(),
        }));

        let inbox = match &follow.actor {
            vocab::Reference::Object(actor) => Some(actor.inbox.clone()),
            vocab::Reference::Id(_) => None,
        };

        if let Some(inbox) = inbox {
            let accept = vocab::Accept::new(
                input.accept_id,
                vocab::Reference::id(self.local_actor.id.clone()),
                vocab::Reference::object(follow),
            );

            actions.push(Action::SendActivity(SendActivity {
                activity: Activity::Accept(accept),
                recipients: Recipients::Inbox(inbox),
            }));
        }

        actions
    }

    fn record_undo_follow(&mut self, input: ReceivedUndoFollow) -> Vec<Action> {
        let undo = input.undo;
        let Some(undo_actor) = reference_id(&undo.actor) else {
            return Vec::new();
        };
        let vocab::Reference::Object(follow) = undo.object else {
            return Vec::new();
        };
        let Some(follower) = reference_id(&follow.actor) else {
            return Vec::new();
        };
        let Some(following) = reference_id(&follow.object) else {
            return Vec::new();
        };

        if undo_actor != follower || following != &self.local_actor.id {
            return Vec::new();
        }

        let relation = Follower {
            follower: follower.clone(),
            following: following.clone(),
        };
        self.followers.retain(|existing| existing != &relation);

        Vec::from([Action::RemoveFollower(RemoveFollower {
            follower: follower.clone(),
            following: following.clone(),
        })])
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
        note.to = input.to;
        note.cc = input.cc;
        note.content = Some(input.content);
        note.media_type = input.media_type;
        note.published = input.published;
        note.url = input.url;

        let mut create = vocab::Create::new(
            input.create_id,
            actor,
            vocab::Reference::object(note.clone()),
        );
        create.to = note.to.clone();
        create.cc = note.cc.clone();

        let recipients = note_recipients(&self.local_actor, &note);

        let object = Object::Note(note);
        self.objects.push(object.clone());
        self.activities.push(Activity::CreateNote(create.clone()));

        let mut actions = Vec::new();

        actions.push(Action::StoreObject(StoreObject { object }));

        for recipient in recipients {
            actions.push(Action::SendActivity(SendActivity {
                activity: Activity::CreateNote(create.clone()),
                recipients: recipient,
            }));
        }

        actions
    }
}

fn note_recipients(local_actor: &vocab::Actor, note: &vocab::Note) -> Vec<Recipients> {
    let mut recipients = Vec::new();

    for address in note.to.iter().chain(note.cc.iter()) {
        let recipient = if address.as_str() == PUBLIC_COLLECTION {
            // Public describes visibility. It cannot receive an activity.
            continue;
        } else if local_actor.followers.as_ref() == Some(address) {
            Recipients::Followers(local_actor.id.clone())
        } else if address == &local_actor.id {
            continue;
        } else {
            Recipients::Actor(address.clone())
        };

        if !recipients.contains(&recipient) {
            recipients.push(recipient);
        }
    }
    recipients
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
    ReceivedUndoFollow(ReceivedUndoFollow),
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

/// Runtime-provided data for handling a received Undo of a Follow.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceivedUndoFollow {
    pub undo: vocab::Undo,
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
    pub to: vocab::References<vocab::Iri>,
    pub cc: vocab::References<vocab::Iri>,
    pub content: String,
    pub media_type: Option<String>,
    pub published: Option<String>,
    pub url: Option<vocab::Iri>,
}

impl Input {
    pub fn received_follow(follow: vocab::Follow, accept_id: vocab::Iri) -> Self {
        Self::ReceivedFollow(ReceivedFollow { follow, accept_id })
    }

    pub fn received_undo_follow(undo: vocab::Undo) -> Self {
        Self::ReceivedUndoFollow(ReceivedUndoFollow { undo })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Follower {
    pub follower: vocab::Iri,
    pub following: vocab::Iri,
}

/// Something the runtime should perform after core handling.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Action {
    StoreFollower(StoreFollower),
    RemoveFollower(RemoveFollower),
    StoreObject(StoreObject),
    SendActivity(SendActivity),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoreFollower {
    pub follower: vocab::Reference<vocab::Actor>,
    pub following: vocab::Reference<vocab::Actor>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoveFollower {
    /// The remote actor ending the follower relation.
    pub follower: vocab::Iri,
    /// The local actor that was followed.
    pub following: vocab::Iri,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoreObject {
    pub object: Object,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SendActivity {
    pub activity: Activity,
    pub recipients: Recipients,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Recipients {
    /// Deliver directly to this inbox.
    Inbox(vocab::Iri),
    /// Deliver to the current followers of this local actor.
    Followers(vocab::Iri),
    Actor(vocab::Iri),
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Activity {
    Accept(vocab::Accept),
    CreateNote(vocab::Create<vocab::Note>),
    Follow(vocab::Follow),
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

    fn received_undo_follow(follow: vocab::Follow, actor_id: &str) -> Input {
        Input::received_undo_follow(vocab::Undo::new(
            iri("https://remote.example/activities/undo/1"),
            vocab::Reference::id(iri(actor_id)),
            vocab::Reference::object(follow),
        ))
    }

    #[test]
    fn core_is_created_with_local_actor_state() {
        let core = core();

        assert_eq!(
            core.state().local_actor().id,
            iri("https://example.com/users/alice")
        );
        assert!(core.state().followers().is_empty());
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
            result.actions[0],
            Action::StoreFollower(StoreFollower {
                follower: vocab::Reference::object(actor("https://remote.example/users/bob")),
                following: vocab::Reference::id(iri("https://example.com/users/alice")),
            })
        );
        let Action::SendActivity(send) = &result.actions[1] else {
            panic!("expected SendActivity action");
        };
        assert_eq!(
            send.recipients,
            Recipients::Inbox(iri("https://remote.example/users/bob/inbox"))
        );

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
    fn received_follow_refreshes_the_stored_follower_actor() {
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
            second_result.actions[0],
            Action::StoreFollower(StoreFollower {
                follower: vocab::Reference::object({
                    let mut actor = actor("https://remote.example/users/bob");
                    actor.inbox = iri("https://remote.example/inboxes/bob");
                    actor
                }),
                following: vocab::Reference::id(iri("https://example.com/users/alice")),
            })
        );

        let Action::SendActivity(send) = &second_result.actions[1] else {
            panic!("expected SendActivity action");
        };
        assert_eq!(
            send.recipients,
            Recipients::Inbox(iri("https://remote.example/inboxes/bob"))
        );

        let Activity::Accept(accept) = &send.activity else {
            panic!("expected Accept activity");
        };
        assert_eq!(accept.id, iri("https://example.com/activities/accept/2"));

        assert_eq!(
            core.state().followers(),
            &[Follower {
                follower: iri("https://remote.example/users/bob"),
                following: iri("https://example.com/users/alice"),
            }]
        );
    }

    #[test]
    fn received_follow_with_actor_id_records_follower_without_accept_delivery() {
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
    }

    #[test]
    fn received_undo_follow_removes_follower() {
        let mut core = core();
        let follow = vocab::Follow::new(
            iri("https://remote.example/activities/follow/1"),
            vocab::Reference::object(actor("https://remote.example/users/bob")),
            vocab::Reference::id(iri("https://example.com/users/alice")),
        );
        let _ = core.handle(received_follow(
            follow.clone(),
            "https://example.com/activities/accept/1",
        ));

        let result = core.handle(received_undo_follow(
            follow,
            "https://remote.example/users/bob",
        ));

        assert_eq!(
            result.actions,
            Vec::from([Action::RemoveFollower(RemoveFollower {
                follower: iri("https://remote.example/users/bob"),
                following: iri("https://example.com/users/alice"),
            })])
        );
        assert!(core.state().followers().is_empty());
    }

    #[test]
    fn received_undo_follow_rejects_actor_that_does_not_own_follow() {
        let mut core = core();
        let follow = vocab::Follow::new(
            iri("https://remote.example/activities/follow/1"),
            vocab::Reference::object(actor("https://remote.example/users/bob")),
            vocab::Reference::id(iri("https://example.com/users/alice")),
        );
        let _ = core.handle(received_follow(
            follow.clone(),
            "https://example.com/activities/accept/1",
        ));

        let result = core.handle(received_undo_follow(
            follow,
            "https://remote.example/users/mallory",
        ));

        assert!(result.is_empty());
        assert_eq!(core.state().followers().len(), 1);
    }

    #[test]
    fn received_undo_follow_emits_idempotent_removal_action() {
        let follow = vocab::Follow::new(
            iri("https://remote.example/activities/follow/1"),
            vocab::Reference::id(iri("https://remote.example/users/bob")),
            vocab::Reference::id(iri("https://example.com/users/alice")),
        );
        let mut core = core();

        let result = core.handle(received_undo_follow(
            follow,
            "https://remote.example/users/bob",
        ));

        assert_eq!(
            result.actions,
            Vec::from([Action::RemoveFollower(RemoveFollower {
                follower: iri("https://remote.example/users/bob"),
                following: iri("https://example.com/users/alice"),
            })])
        );
    }

    #[test]
    fn user_create_note_records_object_and_emits_followers_delivery() {
        let input = UserCreateNote {
            note_id: iri("https://example.com/notes/1"),
            create_id: iri("https://example.com/activities/create/1"),
            actor: vocab::Reference::id(iri("https://example.com/users/alice")),
            to: vocab::References::one(iri("https://www.w3.org/ns/activitystreams#Public")),
            cc: vocab::References::one(iri("https://example.com/users/alice/followers")),
            content: "Hello from Feder.".to_string(),
            media_type: Some("text/html".to_string()),
            published: Some("2026-06-10T00:00:00Z".to_string()),
            url: Some(iri("https://example.com/@alice/1")),
        };

        let mut core = core();
        let result = core.handle(Input::UserCreateNote(input));

        assert_eq!(result.actions.len(), 2);
        assert_eq!(core.state().objects().len(), 1);
        assert_eq!(core.state().activities().len(), 1);

        let Object::Note(note) = &core.state().objects()[0];
        assert_eq!(note.id, iri("https://example.com/notes/1"));
        assert_eq!(
            note.attributed_to,
            Some(vocab::Reference::id(iri("https://example.com/users/alice")))
        );
        assert_eq!(note.content, Some("Hello from Feder.".to_string()));
        assert_eq!(
            note.to,
            vocab::References::one(iri("https://www.w3.org/ns/activitystreams#Public"))
        );
        assert_eq!(
            note.cc,
            vocab::References::one(iri("https://example.com/users/alice/followers"))
        );
        assert_eq!(note.media_type.as_deref(), Some("text/html"));
        assert_eq!(note.published, Some("2026-06-10T00:00:00Z".to_string()));
        assert_eq!(note.url, Some(iri("https://example.com/@alice/1")));

        match &core.state().activities()[0] {
            Activity::CreateNote(create) => {
                assert_eq!(create.id, iri("https://example.com/activities/create/1"));
                assert_eq!(
                    create.actor,
                    vocab::Reference::id(iri("https://example.com/users/alice"))
                );
                assert_eq!(create.to, note.to);
                assert_eq!(create.cc, note.cc);
            }
            Activity::Accept(_) | Activity::Follow(_) => {
                panic!("expected Create<Note> activity")
            }
        }

        assert_eq!(
            result.actions[0],
            Action::StoreObject(StoreObject {
                object: Object::Note(note.clone()),
            })
        );
        let Action::SendActivity(send) = &result.actions[1] else {
            panic!("expected followers delivery action");
        };
        assert_eq!(
            send.recipients,
            Recipients::Followers(iri("https://example.com/users/alice"))
        );
        let Activity::CreateNote(create) = &send.activity else {
            panic!("expected Create<Note> activity");
        };
        assert_eq!(create.id, iri("https://example.com/activities/create/1"));
        let vocab::Reference::Object(created_note) = &create.object else {
            panic!("expected embedded Note object");
        };
        assert_eq!(created_note.id, iri("https://example.com/notes/1"));
    }

    #[test]
    fn mocked_core_flow_accepts_follow_then_delivers_created_note() {
        let mut core = core();
        let follow = vocab::Follow::new(
            iri("https://remote.example/activities/follow/1"),
            vocab::Reference::object(actor("https://remote.example/users/bob")),
            vocab::Reference::id(iri("https://example.com/users/alice")),
        );

        let follow_result = core.handle(received_follow(
            follow,
            "https://example.com/activities/accept/1",
        ));

        assert_eq!(follow_result.actions.len(), 2);
        assert!(matches!(follow_result.actions[0], Action::StoreFollower(_)));
        let Action::SendActivity(accept_delivery) = &follow_result.actions[1] else {
            panic!("expected Accept delivery action");
        };
        assert_eq!(
            accept_delivery.recipients,
            Recipients::Inbox(iri("https://remote.example/users/bob/inbox"))
        );
        assert!(matches!(accept_delivery.activity, Activity::Accept(_)));

        let create_result = core.handle(Input::UserCreateNote(UserCreateNote {
            note_id: iri("https://example.com/notes/1"),
            create_id: iri("https://example.com/activities/create/1"),
            actor: vocab::Reference::id(iri("https://example.com/users/alice")),
            to: vocab::References::new(),
            cc: vocab::References::new(),
            content: "Hello from Feder.".to_string(),
            media_type: None,
            published: Some("2026-06-10T00:00:00Z".to_string()),
            url: None,
        }));

        assert_eq!(create_result.actions.len(), 2);
        assert!(matches!(create_result.actions[0], Action::StoreObject(_)));
        let Action::SendActivity(create_delivery) = &create_result.actions[1] else {
            panic!("expected followers delivery action");
        };
        assert_eq!(
            create_delivery.recipients,
            Recipients::Followers(iri("https://example.com/users/alice"))
        );
        assert!(matches!(create_delivery.activity, Activity::CreateNote(_)));

        assert_eq!(core.state().followers().len(), 1);
        assert_eq!(core.state().objects().len(), 1);
        assert_eq!(core.state().activities().len(), 1);
    }

    #[test]
    fn user_create_note_normalizes_embedded_local_actor_to_local_actor_id() {
        let mut supplied_actor = actor("https://example.com/users/alice");
        supplied_actor.inbox = iri("https://untrusted.example/inbox");

        let input = UserCreateNote {
            note_id: iri("https://example.com/notes/1"),
            create_id: iri("https://example.com/activities/create/1"),
            actor: vocab::Reference::object(supplied_actor),
            to: vocab::References::new(),
            cc: vocab::References::new(),
            content: "Hello from Feder.".to_string(),
            media_type: None,
            published: None,
            url: None,
        };

        let mut core = core();
        let result = core.handle(Input::UserCreateNote(input));

        assert_eq!(result.actions.len(), 2);

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
            to: vocab::References::new(),
            cc: vocab::References::new(),
            content: "Hello from elsewhere.".to_string(),
            media_type: None,
            published: Some("2026-06-10T00:00:00Z".to_string()),
            url: None,
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
