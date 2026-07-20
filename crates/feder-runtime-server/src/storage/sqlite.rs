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

use feder_core::{Action, http_signatures::ActorKeyPair};
use feder_vocab::{Actor, Iri, Reference};
use rusqlite::{Connection, OptionalExtension, params};

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
            CREATE TABLE IF NOT EXISTS keys (
                actor_id TEXT PRIMARY KEY NOT NULL,
                private_key_pem TEXT NOT NULL,
                public_key_pem TEXT NOT NULL
            );
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
                Action::RemoveFollower(action) => {
                    tx.execute(
                        r#"
                        DELETE FROM followers
                        WHERE follower_actor_id = ?1 AND following_actor_id = ?2
                        "#,
                        params![action.follower.as_str(), action.following.as_str()],
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

    fn insert_actor_key_pair(
        &mut self,
        actor_id: &Iri,
        key_pair: &ActorKeyPair,
    ) -> Result<(), StoreError> {
        self.conn.execute(
            r#"
            INSERT INTO keys (actor_id, private_key_pem, public_key_pem)
            VALUES (?1, ?2, ?3)
            "#,
            params![
                actor_id.as_str(),
                key_pair.private_key_pem(),
                key_pair.public_key_pem(),
            ],
        )?;

        Ok(())
    }

    fn load_actor_key_pair(&self, actor_id: &Iri) -> Result<Option<ActorKeyPair>, StoreError> {
        let encoded_keys = self
            .conn
            .query_row(
                r#"
                SELECT private_key_pem, public_key_pem
                FROM keys
                WHERE actor_id = ?1
                "#,
                [actor_id.as_str()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;

        encoded_keys
            .map(|(private_key_pem, public_key_pem)| {
                ActorKeyPair::from_pem(private_key_pem, public_key_pem)
            })
            .transpose()
            .map_err(StoreError::from)
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
    use feder_core::{Action, RemoveFollower, StoreFollower};

    use super::*;

    const PRIVATE_KEY_PEM: &str = include_str!("../../tests/fixtures/rsa-private-key.pem");
    const PUBLIC_KEY_PEM: &str = include_str!("../../tests/fixtures/rsa-public-key.pem");

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

    fn actor_key_pair() -> ActorKeyPair {
        ActorKeyPair::from_pem(PRIVATE_KEY_PEM.to_string(), PUBLIC_KEY_PEM.to_string())
            .expect("valid actor key pair fixture")
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
    fn open_in_memory_initializes_keys_table() {
        let store = SqliteStore::open_in_memory().expect("open in-memory store");

        let columns: Vec<String> = {
            let mut stmt = store
                .conn
                .prepare("PRAGMA table_info(keys)")
                .expect("prepare keys table info query");
            stmt.query_map([], |row| row.get("name"))
                .expect("query keys table info")
                .collect::<Result<_, _>>()
                .expect("collect keys table columns")
        };

        assert_eq!(
            columns,
            vec![
                "actor_id".to_string(),
                "private_key_pem".to_string(),
                "public_key_pem".to_string(),
            ]
        );
    }

    #[test]
    fn actor_key_pair_roundtrips_for_actor() {
        let mut store = SqliteStore::open_in_memory().expect("open in-memory store");
        let actor_id = iri("https://example.com/users/alice");
        let expected = actor_key_pair();

        store
            .insert_actor_key_pair(&actor_id, &expected)
            .expect("insert actor key pair");
        let actual = store
            .load_actor_key_pair(&actor_id)
            .expect("load actor key pair")
            .expect("stored actor key pair");

        assert_eq!(actual, expected);
    }

    #[test]
    fn actor_key_pair_persists_across_store_reopen() {
        let path = std::env::temp_dir().join(format!(
            "feder-actor-key-test-{}-{}.sqlite3",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time after unix epoch")
                .as_nanos()
        ));
        let actor_id = iri("https://example.com/users/alice");
        let expected = actor_key_pair();

        {
            let mut store = SqliteStore::open(&path).expect("open SQLite store");
            store
                .insert_actor_key_pair(&actor_id, &expected)
                .expect("insert actor key pair");
        }

        let store = SqliteStore::open(&path).expect("reopen SQLite store");
        let actual = store
            .load_actor_key_pair(&actor_id)
            .expect("load actor key pair")
            .expect("persisted actor key pair");

        assert_eq!(actual, expected);

        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn load_actor_key_pair_returns_none_for_unknown_actor() {
        let store = SqliteStore::open_in_memory().expect("open in-memory store");

        let key_pair = store
            .load_actor_key_pair(&iri("https://example.com/users/unknown"))
            .expect("load actor key pair");

        assert!(key_pair.is_none());
    }

    #[test]
    fn load_actor_key_pair_rejects_invalid_stored_keys() {
        let store = SqliteStore::open_in_memory().expect("open in-memory store");
        store
            .conn
            .execute(
                r#"
                INSERT INTO keys (actor_id, private_key_pem, public_key_pem)
                VALUES (?1, ?2, ?3)
                "#,
                params![
                    "https://example.com/users/alice",
                    "not a private key",
                    PUBLIC_KEY_PEM,
                ],
            )
            .expect("insert invalid actor key pair");

        let result = store.load_actor_key_pair(&iri("https://example.com/users/alice"));

        assert!(matches!(result, Err(StoreError::ActorKey(_))));
    }

    #[test]
    fn insert_actor_key_pair_refuses_to_replace_existing_key() {
        let mut store = SqliteStore::open_in_memory().expect("open in-memory store");
        let actor_id = iri("https://example.com/users/alice");
        let key_pair = actor_key_pair();

        store
            .insert_actor_key_pair(&actor_id, &key_pair)
            .expect("insert actor key pair");
        let result = store.insert_actor_key_pair(&actor_id, &key_pair);

        assert!(matches!(result, Err(StoreError::Sqlite(_))));
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
    fn persist_actions_removes_follower() {
        let mut store = SqliteStore::open_in_memory().expect("open in-memory store");
        store
            .persist_actions(&[store_follower_action()])
            .expect("persist follower action");

        store
            .persist_actions(&[Action::RemoveFollower(RemoveFollower {
                follower: iri("https://remote.example/users/bob"),
                following: iri("https://example.com/users/alice"),
            })])
            .expect("persist follower removal action");

        let follower_count: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM followers", [], |row| row.get(0))
            .expect("query follower count");
        assert_eq!(follower_count, 0);
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
            .persist_actions(core::slice::from_ref(&action))
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
