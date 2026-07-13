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

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to bind server socket")]
    Bind(#[source] std::io::Error),

    #[error("server failed")]
    Serve(#[source] std::io::Error),

    #[error("storage failed")]
    Storage(#[from] crate::storage::StoreError),
}
