# The gates a pull request must pass. CI runs `just check`. The web app
# builds first so the `arcl` the tests run embeds it. `pnpm lint` checks the
# Prettier formatting of apps/web and packages/ui, before the cargo steps so
# a formatting error fails fast.
check:
    pnpm --filter @arclightning/web build
    pnpm lint
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    pnpm check
    pnpm test

fmt:
    cargo fmt --all
    pnpm format
