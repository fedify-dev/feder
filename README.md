Feder
=====

Embedded-first ActivityPub server framework.


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

Feder separates ActivityPub protocol decisions from platform execution.
`feder-core` derives transient protocol outcomes from application-provided
facts without retaining state. `feder-server` supplies a standard operating
system runtime with HTTP networking, SQLite storage, actor resolution, request
verification, and activity delivery.

The first target is a Linux proof of concept for a small single-user
ActivityPub server. Future runtimes may explore more constrained environments.


License
-------

Feder is licensed under the GNU Affero General Public License v3.0. See
[*LICENSE*](./LICENSE) for details.
