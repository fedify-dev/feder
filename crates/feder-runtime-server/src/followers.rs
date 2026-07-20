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
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use feder_vocab::OrderedCollection;

use crate::{app::AppState, negotiation::accepts_activitypub, storage::RuntimeStore};

/// Return the local actor's followers as a one-shot ordered collection.
pub async fn followers(
    State(app_state): State<AppState>,
    Path(username): Path<String>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    if username != app_state.username {
        return Err(StatusCode::NOT_FOUND);
    }
    if !accepts_activitypub(&headers) {
        return Err(StatusCode::NOT_ACCEPTABLE);
    }

    let followers = app_state
        .store
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .list_followers(&app_state.local_actor.id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let total_items =
        u64::try_from(followers.len()).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let ordered_items = followers
        .into_iter()
        .map(|follower| follower.follower)
        .collect();
    let collection_id = app_state
        .local_actor
        .followers
        .clone()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let collection = OrderedCollection::new(collection_id, total_items, ordered_items);

    Ok((
        [
            (header::CONTENT_TYPE, "application/activity+json"),
            (header::VARY, "Accept"),
        ],
        Json(collection),
    )
        .into_response())
}
