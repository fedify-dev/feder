Feder Runtime Server
====================

Reusable Axum/Tokio server integration for Feder on standard operating
systems.

This crate builds an Axum router from caller-provided runtime configuration.
It provides a health check endpoint, WebFinger discovery, and a local actor
route. The caller chooses concrete bind addresses, actor IRIs, usernames, and
handle hosts.

ActivityPub inbox handling for supported Follow activities is included. Storage,
signature verification, and delivery are intentionally left to later issues.


Example
-------

~~~~ rust
use feder_runtime_server::{InboxAuthPolicy, RuntimeConfig, build_router};

let config = RuntimeConfig {
    bind: "127.0.0.1:3000".parse().expect("valid bind address"),
    actor_id: "http://127.0.0.1:3000/users/alice"
        .parse()
        .expect("valid actor IRI"),
    inbox: "http://127.0.0.1:3000/users/alice/inbox"
        .parse()
        .expect("valid inbox IRI"),
    outbox: "http://127.0.0.1:3000/users/alice/outbox"
        .parse()
        .expect("valid outbox IRI"),
    username: "alice".to_string(),
    handle_host: "127.0.0.1:3000".to_string(),
    inbox_auth_policy: InboxAuthPolicy::AllowUnsignedInsecureDev,
};

let app = build_router(config);
~~~~


Demo
----

A runnable single-user demo lives in `examples/single-user-server`:

~~~~ sh
RUST_LOG=info cargo run -p single-user-server
~~~~
