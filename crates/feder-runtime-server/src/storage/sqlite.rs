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

use feder_core::Action;
use feder_vocab::{Actor, Iri, Reference};
use rusqlite::{Connection, params};

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
            "#,
        )?;

        Ok(())
    }
}

impl RuntimeStore for SqliteStore {
    fn persist_actions(&mut self, actions: &[Action]) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;

        for action in actions {
            match action {
                Action::StoreFollower(action) => {
                    let follower = actor_reference_id(&action.follower);
                    let following = actor_reference_id(&action.following);
                    let inbox = actor_reference_inbox(&action.follower);
                    let shared_inbox = actor_reference_shared_inbox(&action.follower);

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
                            follower.as_str(),
                            following.as_str(),
                            inbox.map(|inbox| inbox.as_str()),
                            shared_inbox.map(|shared_inbox| shared_inbox.as_str()),
                        ],
                    )?;
                }
                Action::StoreDeliveryTarget(action) => {
                    tx.execute(
                        r#"
                        UPDATE followers
                        SET inbox_url = ?2
                        WHERE follower_actor_id = ?1
                        "#,
                        params![action.target.actor.as_str(), action.target.inbox.as_str()],
                    )?;
                }
                _ => {}
            }
        }

        tx.commit()?;

        Ok(())
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
    use feder_core::{Action, StoreFollower};

    use super::*;

    fn iri(value: &str) -> Iri {
        value.parse().expect("valid test IRI")
    }

    fn store_follower_action() -> Action {
        Action::StoreFollower(StoreFollower {
            follower: Reference::id(iri("https://remote.example/users/bob")),
            following: Reference::id(iri("https://example.com/users/alice")),
        })
    }

    fn actor(id: &str) -> Actor {
        Actor::person(
            iri(id),
            iri(&format!("{id}/inbox")),
            iri(&format!("{id}/outbox")),
        )
    }

    #[test]
    fn open_in_memory_initializes_followers_table() {
        let store = SqliteStore::open_in_memory().expect("open in-memory store");

        let table_count: i64 = store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'followers'",
                [],
                |row| row.get(0),
            )
            .expect("query followers table");

        assert_eq!(table_count, 1);

        let columns: Vec<String> = {
            let mut stmt = store
                .conn
                .prepare("PRAGMA table_info(followers)")
                .expect("prepare followers table info query");
            stmt.query_map([], |row| row.get("name"))
                .expect("query followers table info")
                .collect::<Result<_, _>>()
                .expect("collect followers table columns")
        };

        assert!(columns.contains(&"follower_actor_id".to_string()));
        assert!(columns.contains(&"following_actor_id".to_string()));
        assert!(columns.contains(&"inbox_url".to_string()));
        assert!(columns.contains(&"shared_inbox_url".to_string()));

        let index_count: i64 = store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_followers_following_actor_id'",
                [],
                |row| row.get(0),
            )
            .expect("query followers following index");

        assert_eq!(index_count, 1);
    }

    #[test]
    fn persist_actions_stores_follower() {
        let mut store = SqliteStore::open_in_memory().expect("open in-memory store");

        store
            .persist_actions(&[store_follower_action()])
            .expect("persist follower action");

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
    fn persist_actions_stores_embedded_follower_inbox() {
        let mut store = SqliteStore::open_in_memory().expect("open in-memory store");
        let action = Action::StoreFollower(StoreFollower {
            follower: Reference::object(actor("https://remote.example/users/bob")),
            following: Reference::id(iri("https://example.com/users/alice")),
        });

        store
            .persist_actions(&[action])
            .expect("persist follower action");

        let inbox: Option<String> = store
            .conn
            .query_row("SELECT inbox_url FROM followers", [], |row| row.get(0))
            .expect("query stored follower inbox");

        assert_eq!(
            inbox.as_deref(),
            Some("https://remote.example/users/bob/inbox")
        );
    }

    #[test]
    fn persist_actions_stores_embedded_follower_shared_inbox() {
        let mut store = SqliteStore::open_in_memory().expect("open in-memory store");
        let mut follower = actor("https://remote.example/users/bob");
        follower.endpoints = Some(feder_vocab::Endpoints {
            shared_inbox: Some(iri("https://remote.example/inbox")),
        });
        let action = Action::StoreFollower(StoreFollower {
            follower: Reference::object(follower),
            following: Reference::id(iri("https://example.com/users/alice")),
        });

        store
            .persist_actions(&[action])
            .expect("persist follower action");

        let shared_inbox: Option<String> = store
            .conn
            .query_row("SELECT shared_inbox_url FROM followers", [], |row| {
                row.get(0)
            })
            .expect("query stored follower shared inbox");

        assert_eq!(
            shared_inbox.as_deref(),
            Some("https://remote.example/inbox")
        );
    }

    #[test]
    fn persist_actions_ignores_duplicate_follower() {
        let mut store = SqliteStore::open_in_memory().expect("open in-memory store");
        let action = store_follower_action();

        store
            .persist_actions(&[action.clone()])
            .expect("persist follower action first time");
        store
            .persist_actions(&[action])
            .expect("persist follower action second time");

        let follower_count: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM followers", [], |row| row.get(0))
            .expect("query follower count");

        assert_eq!(follower_count, 1);
    }

    #[test]
    fn persist_actions_updates_follower_inbox_from_delivery_target() {
        let mut store = SqliteStore::open_in_memory().expect("open in-memory store");

        store
            .persist_actions(&[store_follower_action()])
            .expect("persist ID-only follower action");
        store
            .persist_actions(&[Action::StoreDeliveryTarget(
                feder_core::StoreDeliveryTarget {
                    target: feder_core::DeliveryTarget {
                        actor: iri("https://remote.example/users/bob"),
                        inbox: iri("https://remote.example/users/bob/updated-inbox"),
                    },
                },
            )])
            .expect("persist delivery target action");

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
    fn list_followers_returns_stored_followers() {
        let mut store = SqliteStore::open_in_memory().expect("open in-memory store");

        store
            .persist_actions(&[store_follower_action()])
            .expect("persist follower action");

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
        let mut follower = actor("https://remote.example/users/bob");
        follower.endpoints = Some(feder_vocab::Endpoints {
            shared_inbox: Some(iri("https://remote.example/inbox")),
        });
        let action = Action::StoreFollower(StoreFollower {
            follower: Reference::object(follower),
            following: Reference::id(iri("https://example.com/users/alice")),
        });

        store
            .persist_actions(&[action])
            .expect("persist follower action");

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
        let bob_follows_alice = Action::StoreFollower(StoreFollower {
            follower: Reference::id(iri("https://remote.example/users/bob")),
            following: Reference::id(iri("https://example.com/users/alice")),
        });
        let carol_follows_eve = Action::StoreFollower(StoreFollower {
            follower: Reference::id(iri("https://remote.example/users/carol")),
            following: Reference::id(iri("https://example.com/users/eve")),
        });

        store
            .persist_actions(&[bob_follows_alice, carol_follows_eve])
            .expect("persist follower actions");

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
        let mut follower = actor("https://remote.example/users/bob");
        follower.endpoints = Some(feder_vocab::Endpoints {
            shared_inbox: Some(iri("https://remote.example/inbox")),
        });
        let follower_with_inbox = Action::StoreFollower(StoreFollower {
            follower: Reference::object(follower),
            following: Reference::id(iri("https://example.com/users/alice")),
        });
        let follower_without_inbox = Action::StoreFollower(StoreFollower {
            follower: Reference::id(iri("https://remote.example/users/carol")),
            following: Reference::id(iri("https://example.com/users/alice")),
        });

        store
            .persist_actions(&[follower_with_inbox, follower_without_inbox])
            .expect("persist follower actions");

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
        let bob_follows_alice = Action::StoreFollower(StoreFollower {
            follower: Reference::object(actor("https://remote.example/users/bob")),
            following: Reference::id(iri("https://example.com/users/alice")),
        });
        let carol_follows_eve = Action::StoreFollower(StoreFollower {
            follower: Reference::object(actor("https://remote.example/users/carol")),
            following: Reference::id(iri("https://example.com/users/eve")),
        });

        store
            .persist_actions(&[bob_follows_alice, carol_follows_eve])
            .expect("persist follower actions");

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
