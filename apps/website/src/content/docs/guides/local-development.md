---
title: Local Development
description: Build the CLI and preview the Arc Lightning documentation site locally.
---

Arc Lightning is a Rust CLI with a SvelteKit and mdsvex documentation site.
The repository keeps both projects side by side so CLI behavior and user-facing
usage can be checked together.

## Build the CLI

From the repository root:

```sh
cargo build
cargo run -- --help
```

Run the project checks before opening a change for review:

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Use `cargo run --` to try a command from the checkout without installing a
binary. For example:

```sh
cargo run -- init
cargo run -- --json idea list
```

Run those commands inside a temporary or real non-bare Git worktree. Arc
Lightning discovers the worktree root before it opens `.arcl/arcl.db`.

## Run the docs site

Install the workspace dependencies once from the repository root:

```sh
pnpm install
```

Start the SvelteKit development server:

```sh
pnpm dev:website
```

Build and preview the production output with:

```sh
pnpm --filter @arclightning/website build
pnpm --filter @arclightning/website preview
```

## Edit documentation

Content lives in `apps/website/src/content/docs/`:

- `index.mdx` is the splash page;
- `overview.md` explains the work model;
- `quick-start.md` gets a new user to a working tracker;
- `guides/` contains task-oriented workflows; and
- `reference/manual.md` lists the CLI surface and output contracts.

The SvelteKit content loader discovers Markdown pages under this directory.
Keep examples executable against the current CLI, and update the README when a
command’s user-facing behavior changes.

## Edit the theme

`apps/website/src/styles/theme.css` composes the shared Arc Lightning tokens with
the documentation layout. The shared package supplies IBM Plex Sans for body
copy, Google Sans for headings and interface labels, and Google Sans Code for
code. Keep website-only navigation and article styles in the website.
