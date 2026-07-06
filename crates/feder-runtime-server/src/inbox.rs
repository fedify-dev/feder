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
    http::{HeaderMap, Method, Request, StatusCode, Uri, header::CONTENT_TYPE},
    response::{IntoResponse, Response},
};

use crate::app::AppState;

pub struct InboxRequest {
    pub username: String,
    pub headers: HeaderMap,
    pub method: Method,
    pub uri: Uri,
    pub body: Bytes,
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

    Ok(StatusCode::ACCEPTED.into_response())
}
