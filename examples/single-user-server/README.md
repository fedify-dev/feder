Single-User Server Example
==========================

Demo app using `feder-runtime-server` with one hardcoded local actor.

The example chooses concrete runtime values for the reusable server crate:

 -  actor: `http://127.0.0.1:3000/users/alice`
 -  bind address: `127.0.0.1:3000`
 -  storage: in-memory SQLite
 -  inbox auth policy: unsigned requests allowed for local development


Run
---

On Unix shells:

~~~~ sh
RUST_LOG=info cargo run -p single-user-server
~~~~

On PowerShell:

~~~~ powershell
$env:RUST_LOG = "info"; cargo run -p single-user-server
~~~~

On cmd.exe:

~~~~ bat
set RUST_LOG=info && cargo run -p single-user-server
~~~~

The demo actor is:

~~~~ text
http://127.0.0.1:3000/users/alice
~~~~

The server listens on:

~~~~ text
127.0.0.1:3000
~~~~

Check the process:

~~~~ sh
curl -i http://127.0.0.1:3000/healthz
~~~~

Expected response:

~~~~ text
HTTP/1.1 204 No Content
~~~~

Fetch the local actor:

~~~~ sh
curl -i http://127.0.0.1:3000/users/alice
~~~~

Supported `Follow` activities posted to `/users/alice/inbox` are parsed by the
runtime, decided by `feder-core`, and applied to in-memory storage. Other
activity types are currently accepted and ignored.
