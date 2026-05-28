Contributing to Feder
=====================

Thank you for your interest in contributing to Feder. This document outlines
the development workflow and the tools we use to maintain code quality.

Please also read the project's [AI usage policy](./AI_POLICY.md) before
submitting issues, discussions, pull requests, or commits that use AI
assistance.


Prerequisites
-------------

We use mise to manage our development environment and tasks. Before you start,
ensure you have the following installed:

 -  mise
 -  Rust (managed via mise)

Once mise is installed, you can set up the project by running:

~~~~ bash
mise install
~~~~


Development workflow
--------------------

We use mise tasks to automate common development steps. Please ensure your
changes pass the automated checks before submitting a pull request.

Please open pull requests against the `main` branch of the Feder upstream
repository.

### Code formatting

We use `rustfmt` to maintain a consistent coding style. You can automatically
format your code by running:

~~~~ bash
mise run fmt
~~~~

### Quality checks

Before pushing your changes, run the full suite of quality checks. This
includes type checking, linting with Clippy, and verifying that the code is
correctly formatted.

~~~~ bash
mise run check
~~~~

We configure Clippy to treat all warnings as errors in `Cargo.toml`. This
ensures that the codebase remains clean and free of common pitfalls. If
`mise run check` fails, please address the reported issues before proceeding.


Project direction
-----------------

Feder is still early-stage, so APIs and crate boundaries may change. The
current direction is to build Feder as a Rust framework for ActivityPub
applications with a portable protocol core and platform-specific runtimes.

The main architectural rule is:

> The core decides what should happen. The runtime decides how it happens on a
> specific platform.

In practice, this means protocol decisions should stay separate from platform
execution. The intended crate roles are:

 -  `feder-vocab`: Type-safe representations of Activity Vocabulary objects,
    such as actors, notes, and activities.
 -  `feder-core`: The portable ActivityPub protocol engine, responsible for
    protocol decisions and state transitions.
 -  Runtime crates: Platform-specific execution layers for networking, storage,
    clocks, timers, async runtimes, and operating system or hardware
    integration.

When contributing to `feder-core`, avoid adding direct dependencies on HTTP
clients or servers, databases, filesystems, async runtimes, system clocks, or
platform-specific crates. Runtime crates may use those dependencies when
appropriate, but those choices should not leak into the portable core.

Core behaviour should generally be tested by feeding an input into the core and
asserting the returned actions. Core tests should not require real networking,
storage, or async execution.

### Git pre-commit hook

You can automate the quality checks by registering a Git pre-commit hook. This
will run the `check` task every time you commit.

~~~~ bash
mise generate git-pre-commit --write --task=check
~~~~
