Feder Core
==========

Pure ActivityPub decision logic for Feder.

This crate does not perform HTTP, database, filesystem, clock, or random ID
operations. A runtime provides the state and context needed for one decision,
then applies the returned decision itself.


Core decisions
--------------

~~~~ text
stored state + policy + context + ActivityPub input
    -> state changes + effects
~~~~

