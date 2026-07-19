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

use feder_core::{Action, Activity, SendActivity};
use reqwest::{Client, StatusCode, redirect::Policy};

#[derive(Clone, Debug)]
pub struct ActivitySender {
    client: Client,
}

impl ActivitySender {
    pub fn new() -> Result<Self, SendError> {
        let client = Client::builder()
            .redirect(Policy::none())
            .build()
            .map_err(SendError::BuildClient)?;

        Ok(Self { client })
    }

    pub async fn send_actions(&self, actions: &[Action]) -> Result<(), SendError> {
        let mut first_error = None;

        for action in actions {
            if let Action::SendActivity(send) = action
                && let Err(error) = self.send(send).await
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }

        first_error.map_or(Ok(()), Err)
    }

    async fn send(&self, send: &SendActivity) -> Result<(), SendError> {
        let body = match &send.activity {
            Activity::Accept(activity) => serde_json::to_vec(activity),
            Activity::CreateNote(activity) => serde_json::to_vec(activity),
            _ => return Err(SendError::UnsupportedActivity),
        }
        .map_err(SendError::Serialize)?;

        let response = self
            .client
            .post(send.inbox.as_str())
            .header(reqwest::header::CONTENT_TYPE, "application/activity+json")
            .body(body)
            .send()
            .await
            .map_err(SendError::Request)?;

        if !response.status().is_success() {
            return Err(SendError::UnsuccessfulStatus {
                inbox: send.inbox.to_string(),
                status: response.status(),
            });
        }

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SendError {
    #[error("failed to build HTTP client")]
    BuildClient(#[source] reqwest::Error),

    #[error("failed to serialize activity")]
    Serialize(#[source] serde_json::Error),

    #[error("failed to send activity")]
    Request(#[source] reqwest::Error),

    #[error("sending activity to {inbox} returned {status}")]
    UnsuccessfulStatus { inbox: String, status: StatusCode },

    #[error("activity type is not supported for sending")]
    UnsupportedActivity,
}
