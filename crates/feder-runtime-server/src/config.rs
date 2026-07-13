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

use std::{net::SocketAddr, path::PathBuf};

use feder_vocab::Iri;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InboxAuthPolicy {
    RequireSigned,
    AllowUnsignedInsecureDev,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageConfig {
    InMemory,
    Sqlite { path: PathBuf },
}

pub struct RuntimeConfig {
    pub bind: SocketAddr,
    pub actor_id: Iri,
    pub inbox: Iri,
    pub outbox: Iri,
    pub username: String,
    pub handle_host: String,
    pub inbox_auth_policy: InboxAuthPolicy,
    pub storage: StorageConfig,
}

#[cfg(test)]
pub(crate) fn test_config() -> RuntimeConfig {
    RuntimeConfig {
        actor_id: "http://127.0.0.1:3000/users/alice"
            .parse()
            .expect("valid actor IRI"),
        inbox: "http://127.0.0.1:3000/users/alice/inbox"
            .parse()
            .expect("valid inbox IRI"),
        outbox: "http://127.0.0.1:3000/users/alice/outbox"
            .parse()
            .expect("valid outbox IRI"),
        bind: "127.0.0.1:3000".parse().expect("valid bind address"),
        username: "alice".to_string(),
        handle_host: "127.0.0.1:3000".to_string(),
        inbox_auth_policy: InboxAuthPolicy::AllowUnsignedInsecureDev,
        storage: StorageConfig::InMemory,
    }
}
