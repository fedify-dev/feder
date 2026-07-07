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
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, Method, StatusCode, Uri, header::CONTENT_TYPE},
    response::{IntoResponse, Response},
};

use feder_core::Input;
use feder_vocab::Follow;
use serde_json::{Value, from_slice, from_value};

use crate::app::AppState;

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

    let activity_type = value.get("type").and_then(|value| value.as_str());

    // Unsupported activity types will be ignored
    if activity_type != Some("Follow") {
        return Ok(StatusCode::ACCEPTED.into_response());
    }
    let follow: Follow = from_value(value).map_err(|_| StatusCode::BAD_REQUEST)?;
    let accept_id = accept_id_for_follow(&app_state.local_actor.id, &follow.id)?;
    let input = Input::received_follow(follow, accept_id);

    let mut core = app_state
        .core
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let _result = core.handle(input);

    Ok(StatusCode::ACCEPTED.into_response())
}
