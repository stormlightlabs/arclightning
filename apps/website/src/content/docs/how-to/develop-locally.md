---
title: Develop arcl locally
description: Build and run arcl from a checkout, run the repository gates, and preview and edit this documentation site.
---

The repository holds the Rust crates, the web app in `apps/web` and this site
in `apps/website`, so a command and its documentation change in one pull
request.

## Build and run the CLI

From the repository root:

```sh
cargo build
cargo run -- --help
```

Try a command from the checkout without installing a binary. Run it inside a
scratch directory or Git worktree, since arcl finds the project from the
current directory:

```sh
cargo run -- init
cargo run -- --json ready
```

To build `arcl` with the web app that `arcl serve` opens, build the app first:

```sh
pnpm install
pnpm --filter @arclightning/web build
cargo build
```

## Run the gates

Run the gates before asking for review:

```sh
just check
```

`just check` runs the same recipe as CI: the web app build, Prettier,
`cargo fmt`, Clippy with warnings denied, the Rust and TypeScript tests, and
the site build. [`apps/web/README.md`](https://github.com/stormlightlabs/arclightning/blob/main/apps/web/README.md)
covers the web app's browser tests.

## Preview the docs site

```sh
pnpm install
pnpm dev:website
```

Build and preview the production output:

```sh
pnpm --filter @arclightning/website build
pnpm --filter @arclightning/website preview
```

The build prerenders every page and redirect, and fails on a broken internal
link or anchor. `pnpm --filter @arclightning/website test:e2e` builds the site
and runs the Playwright tests against it.

## Edit the documentation

Pages live in `apps/website/src/content/docs/` as Markdown, with `title` and
`description` front matter. The site uses both for the page metadata, the
sitemap and `llms.txt`. Each directory is one section:

| Directory      | Section       | A page                                            |
| -------------- | ------------- | ------------------------------------------------- |
| `tutorials/`   | Tutorials     | walks a learner through one path, start to finish |
| `how-to/`      | How-to guides | gives the steps for one goal                      |
| `explanation/` | Explanation   | says how a part of arcl works and why             |
| `reference/`   | Reference     | lists commands, fields, formats and limits        |

Keep instructions and explanation on separate pages, and link between them.

`apps/website/src/lib/content.ts` sets the sidebar order and maps retired
URLs to the pages that replace them. Run commands for examples in a scratch
project and paste their real output, and check flags against
`arcl <command> --help`.

## Edit the theme

`apps/website/src/styles/theme.css` composes the shared Arc Lightning tokens
with the documentation layout. The shared package supplies IBM Plex Sans for
body copy, Google Sans for headings and interface labels, and Google Sans Code
for code. Keep website-only navigation and article styles in the website.
