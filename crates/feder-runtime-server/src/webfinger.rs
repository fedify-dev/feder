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
    extract::{Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};

use crate::app::AppState;

#[derive(Deserialize)]
pub struct WebFingerQuery {
    resource: Option<String>,
}

#[derive(Serialize)]
pub struct WebFingerResponse {
    subject: String,
    aliases: Vec<String>,
    links: Vec<WebFingerLink>,
}

#[derive(Serialize)]
pub struct WebFingerLink {
    rel: &'static str,
    #[serde(rename = "type")]
    media_type: &'static str,
    href: String,
}
pub async fn webfinger(
    State(state): State<AppState>,
    Query(query): Query<WebFingerQuery>,
) -> Result<Json<WebFingerResponse>, StatusCode> {
    let Some(resource) = query.resource else {
        return Err(StatusCode::BAD_REQUEST);
    };

    let expected = format!("acct:{}@{}", state.username, state.handle_host);
    if resource != expected {
        return Err(StatusCode::NOT_FOUND);
    }

    let actor_id = state.actor_id.to_string();

    Ok(Json(WebFingerResponse {
        subject: resource,
        aliases: vec![actor_id.clone()],
        links: vec![WebFingerLink {
            rel: "self",
            media_type: "application/activity+json",
            href: actor_id,
        }],
    }))
}
