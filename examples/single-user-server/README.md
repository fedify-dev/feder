Single-User Server Example
==========================

Demo app using `feder-runtime-server` with one hardcoded local actor.


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
