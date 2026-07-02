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

use crate::app::AppState;

pub async fn note(
    State(app_state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, StatusCode> {
    // TODO(#25): Replace this seeded preview note with durable runtime storage.
    let note = app_state
        .notes
        .iter()
        .find(|note| note.id.as_str() == format!("http://{}/notes/{id}", app_state.handle_host))
        .cloned()
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok((
        [(header::CONTENT_TYPE, "application/activity+json")],
        Json(note),
    )
        .into_response())
}
