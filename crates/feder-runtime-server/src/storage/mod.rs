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

pub mod sqlite;

use feder_core::{Action, Follower};
pub use sqlite::SqliteStore;

#[derive(Debug, Default)]
pub struct StoredState {
    pub followers: Vec<Follower>,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("sqlite error")]
    Sqlite(#[from] rusqlite::Error),

    #[error("json error")]
    Json(#[from] serde_json::Error),

    #[error("invalid IRI: {0}")]
    InvalidIri(String),
}

pub trait RuntimeStore {
    fn persist_actions(&mut self, actions: &[Action]) -> Result<(), StoreError>;

    fn load_state(&self) -> Result<StoredState, StoreError>;
}
