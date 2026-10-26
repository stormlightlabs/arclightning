# The gates a pull request must pass. CI runs `just check`. The web app
# builds first so the `arcl` the tests run embeds it. `pnpm lint` checks the
# Prettier formatting of apps/web and packages/ui.
check:
    pnpm --filter @arclightning/web build
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    pnpm lint
    pnpm check
    pnpm test

fmt:
    cargo fmt --all
    pnpm format
