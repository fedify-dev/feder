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

use crate::storage::{RuntimeStore, StoreError, StoredState};

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
                PRIMARY KEY (follower_actor_id, following_actor_id)
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

                    tx.execute(
                        r#"
                        INSERT OR IGNORE INTO followers (
                            follower_actor_id,
                            following_actor_id
                        )
                        VALUES (?1, ?2)
                        "#,
                        params![follower.as_str(), following.as_str()],
                    )?;
                }
                _ => {}
            }
        }

        tx.commit()?;

        Ok(())
    }

    fn load_state(&self) -> Result<StoredState, StoreError> {
        Ok(StoredState::default())
    }
}

fn actor_reference_id(reference: &Reference<Actor>) -> &Iri {
    match reference {
        Reference::Id(id) => id,
        Reference::Object(actor) => &actor.id,
    }
}
