Feder
=====

One ActivityPub core, many runtimes.

Feder is an early-stage Rust project for building ActivityPub applications from
a portable protocol core and platform-specific runtimes.


Motivation
----------

Feder grew out of work in the Fedify ecosystem and a question about smaller,
cheaper, and more portable fediverse software. What would it take for a
single-user ActivityPub server to run outside the usual VPS-shaped web
application?

One long-term direction is embedded or device-like federation: not moving a full
Mastodon-style server onto a microcontroller, but decomposing ActivityPub
software so different parts can run on machines with very different resources.


Approach
--------

Feder separates ActivityPub protocol logic from platform execution. The core
should contain deterministic protocol decisions. Runtimes provide
platform-specific pieces such as networking, storage, clocks, scheduling, and
execution.

The first target is a Linux proof of concept for a small single-user
ActivityPub server. Future runtimes may explore more constrained environments.


License
-------

Feder is licensed under the GNU Affero General Public License v3.0. See
[*LICENSE*](./LICENSE) for details.
