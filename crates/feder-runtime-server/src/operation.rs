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

use std::collections::HashSet;

use feder_core::{Action, HandleResult, Input, Recipients, SendActivity, UserCreateNote};

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
        {
            let mut store = self
                .store
                .lock()
                .map_err(|_| Error::StorageStateUnavailable)?;
            store.persist_actions(&result.actions)?;
        };
        let deliveries = self.resolve_outbound_deliveries(&result.actions).await?;

        self.activity_sender.send_actions(&deliveries).await?;

        Ok(result)
    }

    async fn resolve_outbound_deliveries(
        &self,
        actions: &[Action],
    ) -> Result<Vec<SendActivity>, Error> {
        let mut resolved = Vec::new();

        for action in actions {
            if let Action::SendActivity(send) = action {
                match &send.recipients {
                    Recipients::Inbox(_) => resolved.push(send.clone()),
                    Recipients::Followers(actor_id) => {
                        let mut seen_inboxes = HashSet::new();
                        let recipients = {
                            let store = self
                                .store
                                .lock()
                                .map_err(|_| Error::StorageStateUnavailable)?;
                            store.list_follower_recipients(actor_id)?
                        };
                        for recipient in recipients {
                            let inbox = recipient.shared_inbox.unwrap_or(recipient.inbox);
                            if seen_inboxes.insert(inbox.clone()) {
                                resolved.push(SendActivity {
                                    activity: send.activity.clone(),
                                    recipients: Recipients::Inbox(inbox),
                                });
                            }
                        }
                    }
                    Recipients::Actor(actor_id) => {
                        let actor = self.actor_resolver.resolve(actor_id).await?;

                        resolved.push(SendActivity {
                            activity: send.activity.clone(),
                            recipients: Recipients::Inbox(actor.inbox),
                        })
                    }
                }
            }
        }

        Ok(resolved)
    }
}
