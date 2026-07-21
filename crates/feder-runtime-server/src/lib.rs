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

pub mod actor;
pub mod app;
pub mod config;
pub mod error;
pub mod followers;
pub mod inbox;
mod negotiation;
pub mod object;
mod operation;
pub mod send;
pub mod storage;
mod url;
pub mod webfinger;

pub use actor::{ActorResolveError, ActorResolver};
pub use app::{AppState, build_router};
pub use config::{InboxAuthPolicy, OutboundAddressPolicy, RuntimeConfig, StorageConfig};
pub use error::Error;
