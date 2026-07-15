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

use alloc::vec::Vec;

pub use feder_vocab as vocab;

/// Portable core decision logic.
#[derive(Debug, Default)]
pub struct FederCore;

impl FederCore {
    #[must_use]
    pub fn new() -> Self {
        Self
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
