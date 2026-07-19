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

/// Controls which network addresses may receive outgoing activities.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OutboundAddressPolicy {
    /// Allows only publicly routable destination addresses.
    #[default]
    PublicOnly,

    /// Allows private and special-use destinations. This disables SSRF protection.
    AllowPrivateAddress,
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
    pub outbound_address_policy: OutboundAddressPolicy,
    pub storage: StorageConfig,
}
