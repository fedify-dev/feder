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

use feder_core::{HandleResult, Input, UserCreateNote};

use crate::{Error, app::AppState, storage::RuntimeStore};

impl AppState {
    /// Create, persist, and deliver a Note initiated by the local application.
    ///
    /// Persistence occurs before delivery. If delivery fails, this returns an
    /// error while the created Note remains available from the runtime store.
    pub async fn create_note(&self, input: UserCreateNote) -> Result<HandleResult, Error> {
        self.handle_input(Input::UserCreateNote(input)).await
    }

    pub(crate) async fn handle_input(&self, input: Input) -> Result<HandleResult, Error> {
        let result = {
            let mut core = self.core.lock().map_err(|_| Error::CoreStateUnavailable)?;
            core.handle(input)
        };

        self.store
            .lock()
            .map_err(|_| Error::StorageStateUnavailable)?
            .persist_actions(&result.actions)?;

        self.activity_sender.send_actions(&result.actions).await?;

        Ok(result)
    }
}
