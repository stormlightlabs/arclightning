//! MCP adapter for the Arc Lightning application.
//!
//! [`server::ArclServer`] serves typed tools over stdio. Each tool calls the
//! application operations in `arcl-store` directly, with the workspace sync
//! the CLI runs, and returns the CLI's versioned JSON envelopes.

pub mod error;
pub mod project;
pub mod server;
pub mod tools;
