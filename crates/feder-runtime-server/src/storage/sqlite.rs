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

use std::path::Path;

use feder_core::{
    Activity, Decision, Effect, FollowRelationship, Object, ReceivedFollowState, RemoteActorState,
    StateChange,
};
use feder_vocab::{Actor, Iri, Reference};
use rusqlite::{Connection, Transaction, params};

use crate::storage::{RuntimeStore, StoreError, StoredFollower, StoredRecipient};

pub struct SqliteStore {
    conn: Connection,
}

impl SqliteStore {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let store = Self {
            conn: Connection::open(path)?,
        };

        store.init()?;

        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self, StoreError> {
        let store = Self {
            conn: Connection::open_in_memory()?,
        };

        store.init()?;

        Ok(store)
    }

    pub fn init(&self) -> Result<(), StoreError> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS followers (
                follower_actor_id TEXT NOT NULL,
                following_actor_id TEXT NOT NULL,
                inbox_url TEXT,
                shared_inbox_url TEXT,
                PRIMARY KEY (follower_actor_id, following_actor_id)
            );
            CREATE INDEX IF NOT EXISTS idx_followers_following_actor_id
                ON followers (following_actor_id);

            CREATE TABLE IF NOT EXISTS processed_inbox_activities (
                activity_id TEXT PRIMARY KEY
            );

            CREATE TABLE IF NOT EXISTS activities (
                activity_id TEXT PRIMARY KEY,
                activity_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS objects (
                object_id TEXT PRIMARY KEY,
                object_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS outgoing_deliveries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                activity_id TEXT NOT NULL,
                activity_json TEXT NOT NULL,
                inbox_url TEXT NOT NULL
            );
            "#,
        )?;

        Ok(())
    }
}

impl RuntimeStore for SqliteStore {
    fn apply_decision(&mut self, decision: &Decision) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;

        for change in &decision.state_changes {
            apply_state_change(&tx, change)?;
        }

        for effect in &decision.effects {
            apply_effect(&tx, effect)?;
        }

        tx.commit()?;

        Ok(())
    }

    fn load_received_follow_state(
        &self,
        follow: &feder_vocab::Follow,
        local_actor_id: &Iri,
    ) -> Result<ReceivedFollowState, StoreError> {
        let already_processed = self.conn.query_row(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM processed_inbox_activities
                WHERE activity_id = ?1
            )
            "#,
            [follow.id.as_str()],
            |row| row.get::<_, bool>(0),
        )?;

        let follower_id = actor_reference_id(&follow.actor);
        let stored_follower = self.load_stored_follower(follower_id, local_actor_id)?;
        let relationship = if stored_follower.is_some() {
            FollowRelationship::Following
        } else {
            FollowRelationship::NotFollowing
        };

        let embedded_inbox = actor_reference_inbox(&follow.actor).cloned();
        let embedded_shared_inbox = actor_reference_shared_inbox(&follow.actor).cloned();
        let stored_inbox = stored_follower
            .as_ref()
            .and_then(|follower| follower.inbox.clone());
        let stored_shared_inbox = stored_follower
            .as_ref()
            .and_then(|follower| follower.shared_inbox.clone());

        Ok(ReceivedFollowState {
            already_processed,
            relationship,
            remote_actor: Some(RemoteActorState {
                actor_id: follower_id.clone(),
                inbox: embedded_inbox.or(stored_inbox),
                shared_inbox: embedded_shared_inbox.or(stored_shared_inbox),
            }),
        })
    }

    fn list_followers(&self, actor_id: &Iri) -> Result<Vec<StoredFollower>, StoreError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT follower_actor_id, following_actor_id, inbox_url, shared_inbox_url
            FROM followers
            WHERE following_actor_id = ?1
            ORDER BY follower_actor_id, following_actor_id
            "#,
        )?;
        let rows = stmt.query_map([actor_id.as_str()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?;

        rows.map(|row| {
            let (follower, following, inbox, shared_inbox) = row?;
            Ok(StoredFollower {
                follower: parse_iri(follower)?,
                following: parse_iri(following)?,
                inbox: parse_optional_iri(inbox)?,
                shared_inbox: parse_optional_iri(shared_inbox)?,
            })
        })
        .collect()
    }

    fn list_follower_recipients(&self, actor_id: &Iri) -> Result<Vec<StoredRecipient>, StoreError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT follower_actor_id, inbox_url, shared_inbox_url
            FROM followers
            WHERE following_actor_id = ?1
              AND inbox_url IS NOT NULL
            ORDER BY follower_actor_id
            "#,
        )?;
        let rows = stmt.query_map([actor_id.as_str()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?;

        rows.map(|row| {
            let (actor_id, inbox, shared_inbox) = row?;
            Ok(StoredRecipient {
                actor_id: parse_iri(actor_id)?,
                inbox: parse_iri(inbox)?,
                shared_inbox: parse_optional_iri(shared_inbox)?,
            })
        })
        .collect()
    }
}

impl SqliteStore {
    fn load_stored_follower(
        &self,
        follower: &Iri,
        following: &Iri,
    ) -> Result<Option<StoredFollower>, StoreError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT follower_actor_id, following_actor_id, inbox_url, shared_inbox_url
            FROM followers
            WHERE follower_actor_id = ?1
              AND following_actor_id = ?2
            "#,
        )?;
        let mut rows = stmt.query(params![follower.as_str(), following.as_str()])?;

        let Some(row) = rows.next()? else {
            return Ok(None);
        };

        Ok(Some(StoredFollower {
            follower: parse_iri(row.get::<_, String>(0)?)?,
            following: parse_iri(row.get::<_, String>(1)?)?,
            inbox: parse_optional_iri(row.get::<_, Option<String>>(2)?)?,
            shared_inbox: parse_optional_iri(row.get::<_, Option<String>>(3)?)?,
        }))
    }
}

fn apply_state_change(tx: &Transaction<'_>, change: &StateChange) -> Result<(), StoreError> {
    match change {
        StateChange::RecordProcessedActivity { activity_id } => {
            tx.execute(
                "INSERT INTO processed_inbox_activities (activity_id) VALUES (?1)",
                [activity_id.as_str()],
            )?;
        }
        StateChange::AddFollower {
            local_actor,
            remote_actor,
            inbox,
            shared_inbox,
        } => {
            tx.execute(
                r#"
                INSERT INTO followers (
                    follower_actor_id,
                    following_actor_id,
                    inbox_url,
                    shared_inbox_url
                )
                VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT(follower_actor_id, following_actor_id) DO UPDATE SET
                    inbox_url = COALESCE(excluded.inbox_url, followers.inbox_url),
                    shared_inbox_url = COALESCE(
                        excluded.shared_inbox_url,
                        followers.shared_inbox_url
                    )
                "#,
                params![
                    remote_actor.as_str(),
                    local_actor.as_str(),
                    inbox.as_ref().map(|inbox| inbox.as_str()),
                    shared_inbox
                        .as_ref()
                        .map(|shared_inbox| shared_inbox.as_str()),
                ],
            )?;
        }
        StateChange::StoreActivity { activity } => {
            let activity_id = activity_id(activity)?;
            tx.execute(
                r#"
                INSERT INTO activities (activity_id, activity_json)
                VALUES (?1, ?2)
                ON CONFLICT(activity_id) DO UPDATE SET
                    activity_json = excluded.activity_json
                "#,
                params![activity_id.as_str(), serialize_activity(activity)?],
            )?;
        }
        StateChange::StoreObject { object } => {
            let object_id = object_id(object)?;
            tx.execute(
                r#"
                INSERT INTO objects (object_id, object_json)
                VALUES (?1, ?2)
                ON CONFLICT(object_id) DO UPDATE SET
                    object_json = excluded.object_json
                "#,
                params![object_id.as_str(), serialize_object(object)?],
            )?;
        }
        _ => return Err(StoreError::UnsupportedDecisionValue("state change")),
    }

    Ok(())
}

fn apply_effect(tx: &Transaction<'_>, effect: &Effect) -> Result<(), StoreError> {
    match effect {
        Effect::PlanDelivery(delivery) => {
            let activity_id = activity_id(&delivery.activity)?;
            tx.execute(
                r#"
                INSERT INTO outgoing_deliveries (
                    activity_id,
                    activity_json,
                    inbox_url
                )
                VALUES (?1, ?2, ?3)
                "#,
                params![
                    activity_id.as_str(),
                    serialize_activity(&delivery.activity)?,
                    delivery.inbox.as_str(),
                ],
            )?;
        }
        _ => return Err(StoreError::UnsupportedDecisionValue("effect")),
    }

    Ok(())
}

fn activity_id(activity: &Activity) -> Result<&Iri, StoreError> {
    match activity {
        Activity::Accept(activity) => Ok(&activity.id),
        Activity::CreateNote(activity) => Ok(&activity.id),
        _ => Err(StoreError::UnsupportedDecisionValue("activity")),
    }
}

fn object_id(object: &Object) -> Result<&Iri, StoreError> {
    match object {
        Object::Note(object) => Ok(&object.id),
        _ => Err(StoreError::UnsupportedDecisionValue("object")),
    }
}

fn serialize_activity(activity: &Activity) -> Result<String, StoreError> {
    match activity {
        Activity::Accept(activity) => Ok(serde_json::to_string(activity)?),
        Activity::CreateNote(activity) => Ok(serde_json::to_string(activity)?),
        _ => Err(StoreError::UnsupportedDecisionValue("activity")),
    }
}

fn serialize_object(object: &Object) -> Result<String, StoreError> {
    match object {
        Object::Note(object) => Ok(serde_json::to_string(object)?),
        _ => Err(StoreError::UnsupportedDecisionValue("object")),
    }
}

fn actor_reference_id(reference: &Reference<Actor>) -> &Iri {
    match reference {
        Reference::Id(id) => id,
        Reference::Object(actor) => &actor.id,
    }
}

fn actor_reference_inbox(reference: &Reference<Actor>) -> Option<&Iri> {
    match reference {
        Reference::Id(_) => None,
        Reference::Object(actor) => Some(&actor.inbox),
    }
}

fn actor_reference_shared_inbox(reference: &Reference<Actor>) -> Option<&Iri> {
    match reference {
        Reference::Id(_) => None,
        Reference::Object(actor) => actor
            .endpoints
            .as_ref()
            .and_then(|endpoints| endpoints.shared_inbox.as_ref()),
    }
}

fn parse_iri(value: String) -> Result<Iri, StoreError> {
    value
        .parse()
        .map_err(|_| StoreError::InvalidIri(value.to_owned()))
}

fn parse_optional_iri(value: Option<String>) -> Result<Option<Iri>, StoreError> {
    value.map(parse_iri).transpose()
}

#[cfg(test)]
mod tests {
    use feder_core::{
        Activity, Decision, Effect, FollowRelationship, PlannedDelivery, StateChange,
    };

    use super::*;

    fn iri(value: &str) -> Iri {
        value.parse().expect("valid test IRI")
    }

    fn add_follower_decision(
        remote_actor: &str,
        local_actor: &str,
        inbox: Option<&str>,
        shared_inbox: Option<&str>,
    ) -> Decision {
        Decision {
            state_changes: vec![StateChange::AddFollower {
                local_actor: iri(local_actor),
                remote_actor: iri(remote_actor),
                inbox: inbox.map(iri),
                shared_inbox: shared_inbox.map(iri),
            }],
            effects: Vec::new(),
        }
    }

    fn bob_follows_alice_decision() -> Decision {
        add_follower_decision(
            "https://remote.example/users/bob",
            "https://example.com/users/alice",
            None,
            None,
        )
    }

    fn actor(id: &str) -> Actor {
        Actor::person(
            iri(id),
            iri(&format!("{id}/inbox")),
            iri(&format!("{id}/outbox")),
        )
    }

    fn follow(actor: Reference<Actor>) -> feder_vocab::Follow {
        feder_vocab::Follow::new(
            iri("https://remote.example/activities/follow-1"),
            actor,
            Reference::id(iri("https://example.com/users/alice")),
        )
    }

    fn accept_activity() -> Activity {
        Activity::Accept(feder_vocab::Accept::new(
            iri("https://example.com/activities/accept-1"),
            Reference::id(iri("https://example.com/users/alice")),
            Reference::object(follow(Reference::id(iri(
                "https://remote.example/users/bob",
            )))),
        ))
    }

    #[test]
    fn apply_decision_stores_follower() {
        let mut store = SqliteStore::open_in_memory().expect("open in-memory store");

        store
            .apply_decision(&bob_follows_alice_decision())
            .expect("apply follower decision");

        let (follower, following): (String, String) = store
            .conn
            .query_row(
                "SELECT follower_actor_id, following_actor_id FROM followers",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("query stored follower");

        assert_eq!(follower, "https://remote.example/users/bob");
        assert_eq!(following, "https://example.com/users/alice");
    }

    #[test]
    fn apply_decision_ignores_duplicate_follower() {
        let mut store = SqliteStore::open_in_memory().expect("open in-memory store");
        let decision = bob_follows_alice_decision();

        store
            .apply_decision(&decision)
            .expect("apply follower decision first time");
        store
            .apply_decision(&decision)
            .expect("apply follower decision second time");

        let follower_count: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM followers", [], |row| row.get(0))
            .expect("query follower count");

        assert_eq!(follower_count, 1);
    }

    #[test]
    fn apply_decision_updates_follower_inbox() {
        let mut store = SqliteStore::open_in_memory().expect("open in-memory store");

        store
            .apply_decision(&bob_follows_alice_decision())
            .expect("apply ID-only follower decision");
        store
            .apply_decision(&add_follower_decision(
                "https://remote.example/users/bob",
                "https://example.com/users/alice",
                Some("https://remote.example/users/bob/updated-inbox"),
                None,
            ))
            .expect("apply updated follower decision");

        let recipients = store
            .list_follower_recipients(&iri("https://example.com/users/alice"))
            .expect("list follower recipients");

        assert_eq!(
            recipients,
            vec![StoredRecipient {
                actor_id: iri("https://remote.example/users/bob"),
                inbox: iri("https://remote.example/users/bob/updated-inbox"),
                shared_inbox: None,
            }]
        );
    }

    #[test]
    fn apply_decision_persists_state_changes_and_queues_delivery() {
        let mut store = SqliteStore::open_in_memory().expect("open in-memory store");
        let activity = accept_activity();
        let decision = Decision {
            state_changes: vec![
                StateChange::RecordProcessedActivity {
                    activity_id: iri("https://remote.example/activities/follow-1"),
                },
                StateChange::AddFollower {
                    local_actor: iri("https://example.com/users/alice"),
                    remote_actor: iri("https://remote.example/users/bob"),
                    inbox: Some(iri("https://remote.example/users/bob/inbox")),
                    shared_inbox: Some(iri("https://remote.example/inbox")),
                },
                StateChange::StoreActivity {
                    activity: activity.clone(),
                },
            ],
            effects: vec![Effect::PlanDelivery(PlannedDelivery {
                activity,
                inbox: iri("https://remote.example/inbox"),
            })],
        };

        store
            .apply_decision(&decision)
            .expect("apply core decision");

        let state = store
            .load_received_follow_state(
                &follow(Reference::id(iri("https://remote.example/users/bob"))),
                &iri("https://example.com/users/alice"),
            )
            .expect("load received follow state");
        assert!(state.already_processed);
        assert_eq!(state.relationship, FollowRelationship::Following);
        assert_eq!(
            state.remote_actor.expect("remote actor state").shared_inbox,
            Some(iri("https://remote.example/inbox"))
        );

        let stored_activity_count: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM activities", [], |row| row.get(0))
            .expect("count stored activities");
        let delivery_count: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM outgoing_deliveries", [], |row| {
                row.get(0)
            })
            .expect("count queued deliveries");
        let delivery_inbox: String = store
            .conn
            .query_row("SELECT inbox_url FROM outgoing_deliveries", [], |row| {
                row.get(0)
            })
            .expect("query queued delivery inbox");

        assert_eq!(stored_activity_count, 1);
        assert_eq!(delivery_count, 1);
        assert_eq!(delivery_inbox, "https://remote.example/inbox");
    }

    #[test]
    fn apply_decision_rolls_back_when_one_state_change_fails() {
        let mut store = SqliteStore::open_in_memory().expect("open in-memory store");
        store
            .conn
            .execute(
                "INSERT INTO processed_inbox_activities (activity_id) VALUES (?1)",
                ["https://remote.example/activities/follow-1"],
            )
            .expect("insert processed inbox activity");

        let decision = Decision {
            state_changes: vec![
                StateChange::AddFollower {
                    local_actor: iri("https://example.com/users/alice"),
                    remote_actor: iri("https://remote.example/users/bob"),
                    inbox: Some(iri("https://remote.example/users/bob/inbox")),
                    shared_inbox: None,
                },
                StateChange::RecordProcessedActivity {
                    activity_id: iri("https://remote.example/activities/follow-1"),
                },
            ],
            effects: Vec::new(),
        };

        let err = store
            .apply_decision(&decision)
            .expect_err("duplicate processed activity should fail");

        assert!(matches!(err, StoreError::Sqlite(_)));
        assert!(
            store
                .list_followers(&iri("https://example.com/users/alice"))
                .expect("list followers")
                .is_empty()
        );
    }

    #[test]
    fn load_received_follow_state_returns_new_relationship_from_embedded_actor() {
        let store = SqliteStore::open_in_memory().expect("open in-memory store");

        let state = store
            .load_received_follow_state(
                &follow(Reference::object(actor("https://remote.example/users/bob"))),
                &iri("https://example.com/users/alice"),
            )
            .expect("load received follow state");

        assert!(!state.already_processed);
        assert_eq!(state.relationship, FollowRelationship::NotFollowing);
        assert_eq!(
            state.remote_actor,
            Some(RemoteActorState {
                actor_id: iri("https://remote.example/users/bob"),
                inbox: Some(iri("https://remote.example/users/bob/inbox")),
                shared_inbox: None,
            })
        );
    }

    #[test]
    fn load_received_follow_state_returns_existing_relationship_from_storage() {
        let mut store = SqliteStore::open_in_memory().expect("open in-memory store");
        store
            .apply_decision(&add_follower_decision(
                "https://remote.example/users/bob",
                "https://example.com/users/alice",
                Some("https://remote.example/users/bob/inbox"),
                Some("https://remote.example/inbox"),
            ))
            .expect("apply follower decision");

        let state = store
            .load_received_follow_state(
                &follow(Reference::id(iri("https://remote.example/users/bob"))),
                &iri("https://example.com/users/alice"),
            )
            .expect("load received follow state");

        assert_eq!(state.relationship, FollowRelationship::Following);
        assert_eq!(
            state.remote_actor,
            Some(RemoteActorState {
                actor_id: iri("https://remote.example/users/bob"),
                inbox: Some(iri("https://remote.example/users/bob/inbox")),
                shared_inbox: Some(iri("https://remote.example/inbox")),
            })
        );
    }

    #[test]
    fn load_received_follow_state_prefers_embedded_actor_endpoints() {
        let mut store = SqliteStore::open_in_memory().expect("open in-memory store");
        store
            .apply_decision(&add_follower_decision(
                "https://remote.example/users/bob",
                "https://example.com/users/alice",
                Some("https://remote.example/users/bob/inbox"),
                None,
            ))
            .expect("apply follower decision");

        let mut updated_actor = actor("https://remote.example/users/bob");
        updated_actor.inbox = iri("https://remote.example/users/bob/updated-inbox");
        updated_actor.endpoints = Some(feder_vocab::Endpoints {
            shared_inbox: Some(iri("https://remote.example/shared-inbox")),
        });
        let state = store
            .load_received_follow_state(
                &follow(Reference::object(updated_actor)),
                &iri("https://example.com/users/alice"),
            )
            .expect("load received follow state");

        assert_eq!(state.relationship, FollowRelationship::Following);
        assert_eq!(
            state.remote_actor,
            Some(RemoteActorState {
                actor_id: iri("https://remote.example/users/bob"),
                inbox: Some(iri("https://remote.example/users/bob/updated-inbox")),
                shared_inbox: Some(iri("https://remote.example/shared-inbox")),
            })
        );
    }

    #[test]
    fn load_received_follow_state_reports_processed_activity() {
        let store = SqliteStore::open_in_memory().expect("open in-memory store");
        store
            .conn
            .execute(
                "INSERT INTO processed_inbox_activities (activity_id) VALUES (?1)",
                ["https://remote.example/activities/follow-1"],
            )
            .expect("insert processed inbox activity");

        let state = store
            .load_received_follow_state(
                &follow(Reference::id(iri("https://remote.example/users/bob"))),
                &iri("https://example.com/users/alice"),
            )
            .expect("load received follow state");

        assert!(state.already_processed);
    }

    #[test]
    fn list_followers_returns_stored_followers() {
        let mut store = SqliteStore::open_in_memory().expect("open in-memory store");

        store
            .apply_decision(&bob_follows_alice_decision())
            .expect("apply follower decision");

        let followers = store
            .list_followers(&iri("https://example.com/users/alice"))
            .expect("list stored followers");

        assert_eq!(
            followers,
            vec![StoredFollower {
                follower: iri("https://remote.example/users/bob"),
                following: iri("https://example.com/users/alice"),
                inbox: None,
                shared_inbox: None,
            }]
        );
    }

    #[test]
    fn list_followers_returns_follower_inbox() {
        let mut store = SqliteStore::open_in_memory().expect("open in-memory store");

        store
            .apply_decision(&add_follower_decision(
                "https://remote.example/users/bob",
                "https://example.com/users/alice",
                Some("https://remote.example/users/bob/inbox"),
                Some("https://remote.example/inbox"),
            ))
            .expect("apply follower decision");

        let followers = store
            .list_followers(&iri("https://example.com/users/alice"))
            .expect("list stored followers");

        assert_eq!(
            followers,
            vec![StoredFollower {
                follower: iri("https://remote.example/users/bob"),
                following: iri("https://example.com/users/alice"),
                inbox: Some(iri("https://remote.example/users/bob/inbox")),
                shared_inbox: Some(iri("https://remote.example/inbox")),
            }]
        );
    }

    #[test]
    fn list_followers_returns_only_followers_for_actor() {
        let mut store = SqliteStore::open_in_memory().expect("open in-memory store");

        store
            .apply_decision(&Decision {
                state_changes: vec![
                    StateChange::AddFollower {
                        local_actor: iri("https://example.com/users/alice"),
                        remote_actor: iri("https://remote.example/users/bob"),
                        inbox: None,
                        shared_inbox: None,
                    },
                    StateChange::AddFollower {
                        local_actor: iri("https://example.com/users/eve"),
                        remote_actor: iri("https://remote.example/users/carol"),
                        inbox: None,
                        shared_inbox: None,
                    },
                ],
                effects: Vec::new(),
            })
            .expect("apply follower decisions");

        let followers = store
            .list_followers(&iri("https://example.com/users/alice"))
            .expect("list stored followers");

        assert_eq!(
            followers,
            vec![StoredFollower {
                follower: iri("https://remote.example/users/bob"),
                following: iri("https://example.com/users/alice"),
                inbox: None,
                shared_inbox: None,
            }]
        );
    }

    #[test]
    fn list_follower_recipients_returns_followers_with_inboxes() {
        let mut store = SqliteStore::open_in_memory().expect("open in-memory store");

        store
            .apply_decision(&Decision {
                state_changes: vec![
                    StateChange::AddFollower {
                        local_actor: iri("https://example.com/users/alice"),
                        remote_actor: iri("https://remote.example/users/bob"),
                        inbox: Some(iri("https://remote.example/users/bob/inbox")),
                        shared_inbox: Some(iri("https://remote.example/inbox")),
                    },
                    StateChange::AddFollower {
                        local_actor: iri("https://example.com/users/alice"),
                        remote_actor: iri("https://remote.example/users/carol"),
                        inbox: None,
                        shared_inbox: None,
                    },
                ],
                effects: Vec::new(),
            })
            .expect("apply follower decisions");

        let recipients = store
            .list_follower_recipients(&iri("https://example.com/users/alice"))
            .expect("list follower recipients");

        assert_eq!(
            recipients,
            vec![StoredRecipient {
                actor_id: iri("https://remote.example/users/bob"),
                inbox: iri("https://remote.example/users/bob/inbox"),
                shared_inbox: Some(iri("https://remote.example/inbox")),
            }]
        );
    }

    #[test]
    fn list_follower_recipients_returns_only_recipients_for_actor() {
        let mut store = SqliteStore::open_in_memory().expect("open in-memory store");

        store
            .apply_decision(&Decision {
                state_changes: vec![
                    StateChange::AddFollower {
                        local_actor: iri("https://example.com/users/alice"),
                        remote_actor: iri("https://remote.example/users/bob"),
                        inbox: Some(iri("https://remote.example/users/bob/inbox")),
                        shared_inbox: None,
                    },
                    StateChange::AddFollower {
                        local_actor: iri("https://example.com/users/eve"),
                        remote_actor: iri("https://remote.example/users/carol"),
                        inbox: Some(iri("https://remote.example/users/carol/inbox")),
                        shared_inbox: None,
                    },
                ],
                effects: Vec::new(),
            })
            .expect("apply follower decisions");

        let recipients = store
            .list_follower_recipients(&iri("https://example.com/users/alice"))
            .expect("list follower recipients");

        assert_eq!(
            recipients,
            vec![StoredRecipient {
                actor_id: iri("https://remote.example/users/bob"),
                inbox: iri("https://remote.example/users/bob/inbox"),
                shared_inbox: None,
            }]
        );
    }
}
