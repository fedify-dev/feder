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

use std::{sync::Arc, time::SystemTime};

use crate::{config::OutboundAddressPolicy, url};
use feder_core::{
    Activity, Recipients, SendActivity,
    http_signatures::{
        ActorKeyPair, HttpSignatureError, create_sha256_digest_header, sign_draft_cavage,
    },
};
use reqwest::{
    Client, StatusCode, Url,
    header::{CONTENT_TYPE, DATE, HOST},
};

/// Sends core `SendActivity` actions as signed ActivityPub HTTP requests.
#[derive(Clone, Debug)]
pub struct ActivitySender {
    client: Client,
    key_pair: Arc<ActorKeyPair>,
    key_id: String,
    address_policy: OutboundAddressPolicy,
}

impl ActivitySender {
    /// Creates an activity sender for one actor identity.
    pub fn new(
        key_pair: Arc<ActorKeyPair>,
        key_id: String,
        address_policy: OutboundAddressPolicy,
    ) -> Result<Self, SendError> {
        let client = url::build_client(address_policy).map_err(SendError::BuildClient)?;

        Ok(Self {
            client,
            key_pair,
            key_id,
            address_policy,
        })
    }

    /// Attempts every send action and returns the first error encountered.
    pub async fn send_actions(&self, actions: &[SendActivity]) -> Result<(), SendError> {
        let mut first_error = None;

        for action in actions {
            if let Err(error) = self.send(action).await
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
        let Recipients::Inbox(inbox) = &send.recipients else {
            return Err(SendError::UnresolvedRecipients);
        };
        let url =
            Url::parse(inbox.as_str()).map_err(|_| SendError::InvalidInbox(inbox.to_string()))?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(SendError::InvalidInbox(inbox.to_string()));
        }
        crate::url::validate_literal_host(&url, self.address_policy).map_err(|address| {
            SendError::PrivateInboxAddress {
                inbox: inbox.to_string(),
                address,
            }
        })?;
        let mut host = url
            .host()
            .ok_or_else(|| SendError::InvalidInbox(inbox.to_string()))?
            .to_string();
        if let Some(port) = url.port() {
            host = format!("{host}:{port}");
        }
        let date = httpdate::fmt_http_date(SystemTime::now());
        let digest = create_sha256_digest_header(&body);
        let headers = [
            ("content-type", "application/activity+json"),
            ("date", date.as_str()),
            ("digest", digest.as_str()),
            ("host", host.as_str()),
        ];
        let mut request_target = url.path().to_string();
        if let Some(query) = url.query() {
            request_target.push('?');
            request_target.push_str(query);
        }
        let signature = sign_draft_cavage(
            &self.key_pair,
            &self.key_id,
            "POST",
            &request_target,
            &headers,
        )
        .map_err(SendError::Sign)?;

        let response = self
            .client
            .post(url)
            .header(CONTENT_TYPE, "application/activity+json")
            .header(DATE, date)
            .header("Digest", digest)
            .header(HOST, host)
            .header("Signature", signature)
            .body(body)
            .send()
            .await
            .map_err(SendError::Request)?;

        if !response.status().is_success() {
            return Err(SendError::UnsuccessfulStatus {
                inbox: inbox.to_string(),
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

    #[error("invalid recipient inbox: {0}")]
    InvalidInbox(String),

    #[error("recipient inbox {inbox} resolves to non-public address {address}")]
    PrivateInboxAddress {
        inbox: String,
        address: std::net::IpAddr,
    },

    #[error("failed to sign activity request")]
    Sign(#[source] HttpSignatureError),

    #[error("failed to send activity")]
    Request(#[source] reqwest::Error),

    #[error("sending activity to {inbox} returned {status}")]
    UnsuccessfulStatus { inbox: String, status: StatusCode },

    #[error("activity type is not supported for sending")]
    UnsupportedActivity,

    #[error("activity recipients must resolve to an inbox before sending")]
    UnresolvedRecipients,
}
