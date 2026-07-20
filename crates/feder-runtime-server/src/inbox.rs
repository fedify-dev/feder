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

use std::{
    collections::{BTreeMap, HashSet},
    time::{Duration, SystemTime},
};

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, Method, StatusCode, Uri, header::CONTENT_TYPE},
    response::{IntoResponse, Response},
};

use feder_core::{
    Input,
    http_signatures::{create_sha256_digest_header, verify_draft_cavage},
};
use feder_vocab::{Actor, Follow, Iri, Reference, Undo};
use serde_json::{Value, from_slice, from_value};

use crate::app::AppState;
use crate::config::InboxAuthPolicy;
use crate::send::SendError;
use crate::storage::RuntimeStore;

const MAX_SIGNATURE_AGE: Duration = Duration::from_secs(65 * 60);
const MAX_CLOCK_SKEW: Duration = Duration::from_secs(60 * 60);

pub struct InboxRequest {
    pub username: String,
    pub headers: HeaderMap,
    pub method: Method,
    pub uri: Uri,
    pub body: Bytes,
}

fn accept_id_for_follow(
    local_actor_id: &feder_vocab::Iri,
    follow_id: &feder_vocab::Iri,
) -> Result<feder_vocab::Iri, StatusCode> {
    let encoded_follow_id = percent_encoding::utf8_percent_encode(
        follow_id.as_str(),
        percent_encoding::NON_ALPHANUMERIC,
    );

    format!("{local_actor_id}#accepts/{encoded_follow_id}")
        .parse()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn verify_inbox_request(
    app_state: &AppState,
    req: &InboxRequest,
    activity_actor_id: Option<&Iri>,
) -> Result<Option<Actor>, StatusCode> {
    match app_state.inbox_auth_policy {
        InboxAuthPolicy::AllowUnsignedInsecureDev => Ok(None),
        InboxAuthPolicy::RequireSigned => {
            let activity_actor_id = activity_actor_id.ok_or(StatusCode::UNAUTHORIZED)?;
            verify_signed_request(app_state, req, activity_actor_id)
                .await
                .map(Some)
        }
    }
}

async fn verify_signed_request(
    app_state: &AppState,
    req: &InboxRequest,
    activity_actor_id: &Iri,
) -> Result<Actor, StatusCode> {
    let signature_header = req
        .headers
        .get("signature")
        .and_then(|value| value.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let signature = parse_signature_header(signature_header).ok_or(StatusCode::UNAUTHORIZED)?;
    if signature.algorithm != "rsa-sha256"
        || signature.signed_headers.first().map(String::as_str) != Some("(request-target)")
    {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let mut seen_headers = HashSet::new();
    let mut signed_headers = Vec::new();
    for name in signature.signed_headers.iter().skip(1) {
        if name.starts_with('(') || !seen_headers.insert(name.as_str()) {
            return Err(StatusCode::UNAUTHORIZED);
        }
        let values = req.headers.get_all(name).iter().collect::<Vec<_>>();
        let [value] = values.as_slice() else {
            return Err(StatusCode::UNAUTHORIZED);
        };
        let value = value.to_str().map_err(|_| StatusCode::UNAUTHORIZED)?;
        signed_headers.push((name.as_str(), value));
    }
    if !["host", "date", "digest"]
        .iter()
        .all(|required| seen_headers.contains(required))
    {
        return Err(StatusCode::UNAUTHORIZED);
    }

    verify_request_date(&req.headers)?;
    verify_request_digest(&req.headers, &req.body)?;

    let key_id: Iri = signature
        .key_id
        .parse()
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    let public_key = app_state
        .actor_resolver
        .resolve_key(&key_id)
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    if public_key.owner != *activity_actor_id {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let request_target = req
        .uri
        .path_and_query()
        .map_or(req.uri.path(), |value| value.as_str());
    verify_draft_cavage(
        &public_key.public_key_pem,
        req.method.as_str(),
        request_target,
        &signed_headers,
        &signature.signature,
    )
    .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let actor = app_state
        .actor_resolver
        .resolve(activity_actor_id)
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    let actor_owns_key = match actor.public_key.as_ref() {
        Some(Reference::Id(advertised_key_id)) => advertised_key_id == &public_key.id,
        Some(Reference::Object(advertised_key)) => {
            advertised_key.id == public_key.id
                && advertised_key.owner == actor.id
                && advertised_key.public_key_pem == public_key.public_key_pem
        }
        None => false,
    };
    if !actor_owns_key {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(actor)
}

fn activity_actor_id(value: &Value) -> Option<Iri> {
    let actor = value.get("actor")?;
    let actor_id = actor
        .as_str()
        .or_else(|| actor.get("id").and_then(Value::as_str))?;
    actor_id.parse().ok()
}

fn actor_reference_id(reference: &Reference<Actor>) -> &Iri {
    match reference {
        Reference::Id(actor_id) => actor_id,
        Reference::Object(actor) => &actor.id,
    }
}

fn verify_request_date(headers: &HeaderMap) -> Result<(), StatusCode> {
    let date = headers
        .get("date")
        .and_then(|value| value.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)
        .and_then(|value| httpdate::parse_http_date(value).map_err(|_| StatusCode::UNAUTHORIZED))?;
    let now = SystemTime::now();
    if now
        .duration_since(date)
        .is_ok_and(|age| age > MAX_SIGNATURE_AGE)
        || date
            .duration_since(now)
            .is_ok_and(|skew| skew > MAX_CLOCK_SKEW)
    {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(())
}

fn verify_request_digest(headers: &HeaderMap, body: &[u8]) -> Result<(), StatusCode> {
    let digest = headers
        .get("digest")
        .and_then(|value| value.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let expected = create_sha256_digest_header(body);
    let matches = digest.split(',').any(|entry| {
        entry
            .trim()
            .split_once('=')
            .is_some_and(|(algorithm, value)| {
                algorithm.eq_ignore_ascii_case("sha-256")
                    && expected
                        .split_once('=')
                        .is_some_and(|(_, expected)| value == expected)
            })
    });

    if matches {
        Ok(())
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

struct ParsedSignature {
    key_id: String,
    algorithm: String,
    signed_headers: Vec<String>,
    signature: String,
}

fn parse_signature_header(header: &str) -> Option<ParsedSignature> {
    let mut parameters = BTreeMap::new();
    let mut remaining = header;
    while !remaining.trim_start().is_empty() {
        remaining = remaining.trim_start();
        let equals = remaining.find('=')?;
        let name = remaining[..equals].trim();
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return None;
        }
        remaining = &remaining[equals + 1..];
        let (value, rest) = parse_quoted_parameter(remaining.trim_start())?;
        if parameters
            .insert(name.to_ascii_lowercase(), value)
            .is_some()
        {
            return None;
        }
        remaining = rest.trim_start();
        if remaining.is_empty() {
            break;
        }
        remaining = remaining.strip_prefix(',')?;
    }

    let key_id = parameters.remove("keyid")?;
    let algorithm = parameters.remove("algorithm")?;
    let signed_headers = parameters
        .remove("headers")?
        .split_ascii_whitespace()
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    let signature = parameters.remove("signature")?;
    if key_id.is_empty() || signed_headers.is_empty() || signature.is_empty() {
        return None;
    }

    Some(ParsedSignature {
        key_id,
        algorithm: algorithm.to_ascii_lowercase(),
        signed_headers,
        signature,
    })
}

fn parse_quoted_parameter(input: &str) -> Option<(String, &str)> {
    let input = input.strip_prefix('"')?;
    let mut value = String::new();
    let mut escaped = false;
    for (index, character) in input.char_indices() {
        if escaped {
            value.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            return Some((value, &input[index + character.len_utf8()..]));
        } else if character.is_control() {
            return None;
        } else {
            value.push(character);
        }
    }

    None
}

pub async fn inbox(
    State(app_state): State<AppState>,
    Path(username): Path<String>,
    headers: HeaderMap,
    method: Method,
    uri: Uri,
    body: Bytes,
) -> Result<Response, StatusCode> {
    if username != app_state.username {
        return Err(StatusCode::NOT_FOUND);
    }
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");

    if !content_type.starts_with("application/activity+json")
        && !content_type.starts_with("application/ld+json")
    {
        return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    let req = InboxRequest {
        username,
        headers,
        method,
        uri,
        body,
    };

    let value: Value = from_slice(&req.body).map_err(|_| StatusCode::BAD_REQUEST)?;
    let activity_actor_id = activity_actor_id(&value);
    let verified_actor = verify_inbox_request(&app_state, &req, activity_actor_id.as_ref()).await?;

    let activity_type = value.get("type").and_then(|value| value.as_str());

    let input = match activity_type {
        Some("Follow") => {
            let mut follow: Follow = from_value(value).map_err(|_| StatusCode::BAD_REQUEST)?;
            if actor_reference_id(&follow.object) != &app_state.local_actor.id {
                return Ok(StatusCode::ACCEPTED.into_response());
            }
            if let Some(actor) = verified_actor {
                if actor_reference_id(&follow.actor) != &actor.id {
                    return Err(StatusCode::UNAUTHORIZED);
                }
                follow.actor = Reference::object(actor);
            } else {
                app_state
                    .actor_resolver
                    .resolve_reference(&mut follow.actor)
                    .await
                    .map_err(|_| StatusCode::BAD_GATEWAY)?;
            }
            let accept_id = accept_id_for_follow(&app_state.local_actor.id, &follow.id)?;
            Input::received_follow(follow, accept_id)
        }
        Some("Undo") => {
            let undo: Undo = from_value(value).map_err(|_| StatusCode::BAD_REQUEST)?;
            let Reference::Object(follow) = &undo.object else {
                return Ok(StatusCode::ACCEPTED.into_response());
            };
            let undo_actor_id = actor_reference_id(&undo.actor);
            if undo_actor_id != actor_reference_id(&follow.actor) {
                return Err(StatusCode::UNAUTHORIZED);
            }
            if verified_actor
                .as_ref()
                .is_some_and(|actor| undo_actor_id != &actor.id)
            {
                return Err(StatusCode::UNAUTHORIZED);
            }
            if actor_reference_id(&follow.object) != &app_state.local_actor.id {
                return Ok(StatusCode::ACCEPTED.into_response());
            }
            Input::received_undo_follow(undo)
        }
        // Unsupported activity types will be ignored.
        _ => return Ok(StatusCode::ACCEPTED.into_response()),
    };

    let result = {
        let mut core = app_state
            .core
            .lock()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        core.handle(input)
    };

    app_state
        .store
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .persist_actions(&result.actions)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    app_state
        .activity_sender
        .send_actions(&result.actions)
        .await
        .map_err(|error| match error {
            SendError::PrivateInboxAddress { .. }
            | SendError::Request(_)
            | SendError::UnsuccessfulStatus { .. } => StatusCode::BAD_GATEWAY,
            SendError::BuildClient(_)
            | SendError::InvalidInbox(_)
            | SendError::Serialize(_)
            | SendError::Sign(_)
            | SendError::UnsupportedActivity => StatusCode::INTERNAL_SERVER_ERROR,
        })?;

    Ok(StatusCode::ACCEPTED.into_response())
}
