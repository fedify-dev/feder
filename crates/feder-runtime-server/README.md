Feder Runtime Server
====================

Runnable server runtime for Feder on standard operating systems.

This crate currently starts a local development server with one configured
actor and a health check endpoint. ActivityPub discovery, inbox handling,
storage, signing, and delivery are intentionally left to later issues.


Run
---

On Unix shells:

~~~~ sh
RUST_LOG=info cargo run -p feder-runtime-server
~~~~

On PowerShell:

~~~~ powershell
$env:RUST_LOG = "info"; cargo run -p feder-runtime-server
~~~~

On cmd.exe:

~~~~ bat
set RUST_LOG=info && cargo run -p feder-runtime-server
~~~~

The default local actor is:

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
