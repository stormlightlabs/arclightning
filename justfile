# The gates a pull request must pass. CI runs `just check`. The web app
# builds first so the `arcl` the tests run embeds it.
check:
    pnpm --filter @arclightning/web build
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    pnpm check
    pnpm test

fmt:
    cargo fmt --all
