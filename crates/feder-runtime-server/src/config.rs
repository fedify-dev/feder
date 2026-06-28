// Feder: A portable ActivityPub core for many runtimes.
// Copyright (C) 2026 Feder contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use std::net::SocketAddr;

use feder_vocab::Iri;

pub struct RuntimeConfig {
    pub bind: SocketAddr,
    pub actor_id: Iri,
    pub inbox: Iri,
    pub outbox: Iri,
}

impl RuntimeConfig {
    pub fn default_local() -> Self {
        Self {
            actor_id: "http://127.0.0.1:3000/users/alice"
                .parse()
                .expect("valid default actor IRI"),
            inbox: "http://127.0.0.1:3000/users/alice/inbox"
                .parse()
                .expect("valid default inbox IRI"),
            outbox: "http://127.0.0.1:3000/users/alice/outbox"
                .parse()
                .expect("valid default outbox IRI"),
            bind: "127.0.0.1:3000"
                .parse()
                .expect("valid default bind address"),
        }
    }
}
