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

use feder_core::{
    Action, Object,
    http_signatures::{ActorKeyPair, KeyError},
};
use feder_vocab::Iri;
pub use sqlite::SqliteStore;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredFollower {
    pub follower: Iri,
    pub following: Iri,
    pub inbox: Option<Iri>,
    pub shared_inbox: Option<Iri>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredRecipient {
    pub actor_id: Iri,
    pub inbox: Iri,
    pub shared_inbox: Option<Iri>,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("sqlite error")]
    Sqlite(#[from] rusqlite::Error),

    #[error("json error")]
    Json(#[from] serde_json::Error),

    #[error("invalid IRI: {0}")]
    InvalidIri(String),

    #[error("unsupported runtime object type")]
    UnsupportedObjectType,

    #[error("unsupported stored object type: {0}")]
    UnsupportedStoredObjectType(String),

    #[error(transparent)]
    ActorKey(#[from] KeyError),
}

pub trait RuntimeStore {
    fn persist_actions(&mut self, actions: &[Action]) -> Result<(), StoreError>;

    fn list_followers(&self, actor_id: &Iri) -> Result<Vec<StoredFollower>, StoreError>;

    fn list_follower_recipients(&self, actor_id: &Iri) -> Result<Vec<StoredRecipient>, StoreError>;

    fn load_object(&self, object_id: &Iri) -> Result<Option<Object>, StoreError>;

    fn insert_actor_key_pair(
        &mut self,
        actor_id: &Iri,
        key_pair: &ActorKeyPair,
    ) -> Result<(), StoreError>;

    fn load_actor_key_pair(&self, actor_id: &Iri) -> Result<Option<ActorKeyPair>, StoreError>;
}
