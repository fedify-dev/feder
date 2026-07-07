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

use feder_runtime_server::{Error, InboxAuthPolicy, RuntimeConfig, build_router};

fn default_local() -> RuntimeConfig {
    RuntimeConfig {
        bind: "127.0.0.1:3000"
            .parse()
            .expect("valid default bind address"),
        actor_id: "http://127.0.0.1:3000/users/alice"
            .parse()
            .expect("valid default actor IRI"),
        inbox: "http://127.0.0.1:3000/users/alice/inbox"
            .parse()
            .expect("valid default inbox IRI"),
        outbox: "http://127.0.0.1:3000/users/alice/outbox"
            .parse()
            .expect("valid default outbox IRI"),
        username: "alice".to_string(),
        handle_host: "127.0.0.1:3000".to_string(),
        inbox_auth_policy: InboxAuthPolicy::AllowUnsignedInsecureDev,
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = default_local();
    let bind = config.bind;
    let actor_id = config.actor_id.clone();
    let app = build_router(config);

    tracing::info!(bind = %bind, actor = %actor_id, "starting Feder single-user example");

    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .map_err(Error::Bind)?;
    axum::serve(listener, app).await.map_err(Error::Serve)?;

    Ok(())
}
