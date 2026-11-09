---
title: Local Development
description: Build arcl from a checkout, run the repository gates, and preview and edit this documentation site.
---

The repository holds the Rust crates, the web app in `apps/web`, and this
site in `apps/website`, so a change to a command and its documentation land
in one pull request.

## Build the CLI

From the repository root:

```sh
cargo build
cargo run -- --help
```

Run the gates before opening a change for review. `just check` runs the same
recipe as CI: the web app build, Prettier, `cargo fmt`, Clippy with warnings
denied, the Rust and TypeScript tests, and the site build:

```sh
just check
```

Use `cargo run --` to try a command from the checkout without installing a
binary. For example:

```sh
cargo run -- init
cargo run -- --json ready
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

Pages live in `apps/website/src/content/docs/` as Markdown with `title` and
`description` front matter. The site uses both for the page metadata, the
sitemap and `llms.txt`.

- `overview.md` and `quick-start.md` are the Get started pages.
- `guides/` holds task-oriented guides.
- `reference/` holds the manual and other reference pages.

`apps/website/src/lib/content.ts` sets the sidebar order and maps retired
URLs to their replacements; the build prerenders each retired URL as a
redirect. The build also checks every internal link and anchor. Run commands
for examples in a scratch project and paste their real output, and check
flags against `arcl <command> --help`.

## Edit the theme

`apps/website/src/styles/theme.css` composes the shared Arc Lightning tokens with
the documentation layout. The shared package supplies IBM Plex Sans for body
copy, Google Sans for headings and interface labels, and Google Sans Code for
code. Keep website-only navigation and article styles in the website.
