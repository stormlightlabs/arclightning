---
title: Local Development
description: Build the CLI and preview the Arc Lightning documentation site locally.
---

Arc Lightning is a Rust CLI with an Astro Starlight documentation site. The
repository keeps both projects side by side so CLI behavior and user-facing
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

Install the website dependencies once:

```sh
pnpm --dir website install
```

Start the Astro development server:

```sh
pnpm --dir website dev
```

Build and preview the production output with:

```sh
pnpm --dir website build
pnpm --dir website preview
```

## Edit documentation

Content lives in `website/src/content/docs/`:

- `index.mdx` is the splash page;
- `overview.md` explains the work model;
- `quick-start.md` gets a new user to a working tracker;
- `guides/` contains task-oriented workflows; and
- `reference/manual.md` lists the CLI surface and output contracts.

Starlight discovers the guide and reference pages from `website/astro.config.mjs`.
Keep examples executable against the current CLI, and update the README when a
command’s user-facing behavior changes.

## Edit the theme

`website/src/styles/theme.css` contains the site tokens. It keeps the Thunderus
typography and layout values while using the Iceberg dark palette:

- IBM Plex Sans Variable for body and interface text;
- Literata Variable for headings;
- a `48rem` content column; and
- a `1.75` line height for documentation text.

Starlight loads the two Fontsource packages and the theme through
`website/astro.config.mjs`. Keep semantic Starlight variables mapped through
the Arc-prefixed tokens so component styles remain consistent.
