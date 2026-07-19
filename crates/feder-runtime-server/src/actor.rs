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

use axum::{
    Json,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use feder_vocab::{Actor, ActorType, CryptographicKey, Endpoints, Iri, Reference};
use reqwest::{
    Client, StatusCode as HttpStatusCode, Url,
    header::{ACCEPT, CONTENT_TYPE},
};
use serde::Deserialize;

use crate::{app::AppState, config::OutboundAddressPolicy, url};

const MAX_ACTOR_BODY_SIZE: usize = 1_048_576;
const ACTIVITYPUB_ACCEPT: &str = "application/activity+json, application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\"";

pub async fn actor(
    State(app_state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Response, StatusCode> {
    if username != app_state.username {
        return Err(StatusCode::NOT_FOUND);
    }
    let local_actor = app_state.local_actor.clone();

    Ok((
        [(header::CONTENT_TYPE, "application/activity+json")],
        Json(local_actor),
    )
        .into_response())
}

/// Resolves remote ActivityPub actors for runtime protocol handling.
#[derive(Clone, Debug)]
pub struct ActorResolver {
    client: Client,
    address_policy: OutboundAddressPolicy,
}

impl ActorResolver {
    pub fn new(address_policy: OutboundAddressPolicy) -> Result<Self, ActorResolveError> {
        let client = url::build_client(address_policy).map_err(ActorResolveError::BuildClient)?;
        Ok(Self {
            client,
            address_policy,
        })
    }

    pub async fn resolve_reference(
        &self,
        reference: &mut Reference<Actor>,
    ) -> Result<(), ActorResolveError> {
        let Reference::Id(actor_id) = reference else {
            return Ok(());
        };
        let actor_id = actor_id.clone();
        let actor = self.resolve(&actor_id).await?;
        *reference = Reference::object(actor);
        Ok(())
    }

    pub async fn resolve(&self, actor_id: &Iri) -> Result<Actor, ActorResolveError> {
        let url = Url::parse(actor_id.as_str())
            .map_err(|_| ActorResolveError::InvalidActorId(actor_id.to_string()))?;
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.host().is_none()
        {
            return Err(ActorResolveError::InvalidActorId(actor_id.to_string()));
        }
        url::validate_literal_host(&url, self.address_policy).map_err(|address| {
            ActorResolveError::PrivateActorAddress {
                actor: actor_id.to_string(),
                address,
            }
        })?;

        let mut response = self
            .client
            .get(url)
            .header(ACCEPT, ACTIVITYPUB_ACCEPT)
            .send()
            .await
            .map_err(ActorResolveError::Request)?;
        if !response.status().is_success() {
            return Err(ActorResolveError::UnsuccessfulStatus {
                actor: actor_id.to_string(),
                status: response.status(),
            });
        }
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        if !is_activitypub_content_type(content_type) {
            return Err(ActorResolveError::UnsupportedContentType(
                content_type.to_string(),
            ));
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_ACTOR_BODY_SIZE as u64)
        {
            return Err(ActorResolveError::ResponseTooLarge);
        }

        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(ActorResolveError::Request)? {
            if body.len().saturating_add(chunk.len()) > MAX_ACTOR_BODY_SIZE {
                return Err(ActorResolveError::ResponseTooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        let document: ActorDocument =
            serde_json::from_slice(&body).map_err(ActorResolveError::Deserialize)?;
        let actor = document.into_actor();
        if actor.id != *actor_id {
            return Err(ActorResolveError::ActorIdMismatch {
                requested: actor_id.to_string(),
                returned: actor.id.to_string(),
            });
        }

        Ok(actor)
    }
}

fn is_activitypub_content_type(content_type: &str) -> bool {
    let media_type = content_type
        .split_once(';')
        .map_or(content_type, |(media_type, _)| media_type)
        .trim();
    media_type.eq_ignore_ascii_case("application/activity+json")
        || media_type.eq_ignore_ascii_case("application/ld+json")
}

#[derive(Deserialize)]
struct ActorDocument {
    #[serde(rename = "type")]
    kind: ActorType,
    id: Iri,
    inbox: Iri,
    outbox: Iri,
    #[serde(rename = "preferredUsername")]
    preferred_username: Option<String>,
    name: Option<String>,
    endpoints: Option<Endpoints>,
    #[serde(rename = "publicKey")]
    public_key: Option<Reference<CryptographicKey>>,
}

impl ActorDocument {
    fn into_actor(self) -> Actor {
        Actor {
            context: None,
            kind: self.kind,
            id: self.id,
            inbox: self.inbox,
            outbox: self.outbox,
            preferred_username: self.preferred_username,
            name: self.name,
            endpoints: self.endpoints,
            public_key: self.public_key,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ActorResolveError {
    #[error("failed to build actor resolution HTTP client")]
    BuildClient(#[source] reqwest::Error),

    #[error("invalid remote actor ID: {0}")]
    InvalidActorId(String),

    #[error("remote actor {actor} uses non-public address {address}")]
    PrivateActorAddress {
        actor: String,
        address: std::net::IpAddr,
    },

    #[error("failed to fetch remote actor")]
    Request(#[source] reqwest::Error),

    #[error("fetching remote actor {actor} returned {status}")]
    UnsuccessfulStatus {
        actor: String,
        status: HttpStatusCode,
    },

    #[error("remote actor response has unsupported content type: {0}")]
    UnsupportedContentType(String),

    #[error("remote actor response exceeds size limit")]
    ResponseTooLarge,

    #[error("failed to deserialize remote actor")]
    Deserialize(#[source] serde_json::Error),

    #[error("remote actor ID mismatch: requested {requested}, returned {returned}")]
    ActorIdMismatch { requested: String, returned: String },
}
