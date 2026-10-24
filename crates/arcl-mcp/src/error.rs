//! Tool failures, returned in the CLI's version 1 failure envelope.

use rmcp::{
    handler::server::tool::IntoCallToolResult,
    model::{CallToolResponse, CallToolResult},
};
use serde_json::{Value, json};
use thiserror::Error;

use arcl_core::domain::{DomainError, IdError};
use arcl_repo::{vcs::VcsError, workspace::WorkspaceSyncError};
use arcl_store::StorageError;

/// The `format_version` of every failure envelope.
pub const ERROR_FORMAT_VERSION: u8 = 1;

/// A failed tool call.
///
/// A tool returns it as an error result (`isError: true`) whose structured
/// content is `{"format_version": 1, "error": {"code", "message",
/// "exit_code"}}`, the object `arcl --json` writes to standard error. The
/// `exit_code` is the one the CLI would exit with for the same failure.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("{message}")]
pub struct ToolError {
    message: String,
    exit_code: u8,
}

impl ToolError {
    /// A failure in exit category `exit_code`. An invalid project or request
    /// (`3`) gets the CLI's `invalid project or record:` prefix.
    pub fn new(exit_code: u8, message: impl std::fmt::Display) -> Self {
        let message =
            if exit_code == 3 { format!("invalid project or record: {message}") } else { message.to_string() };
        Self { message, exit_code }
    }

    /// An argument the tool cannot use, such as an unknown status or an
    /// out-of-range `limit`.
    pub fn invalid_request(message: impl std::fmt::Display) -> Self {
        Self::new(3, message)
    }

    /// The stable code for the failure: `invalid_request`, `conflict`,
    /// `not_found` or `application_error`.
    pub const fn code(&self) -> &'static str {
        match self.exit_code {
            3 => "invalid_request",
            4 => "conflict",
            5 => "not_found",
            _ => "application_error",
        }
    }

    /// The CLI exit status for the same failure.
    pub const fn exit_code(&self) -> u8 {
        self.exit_code
    }

    /// The failure envelope.
    pub fn envelope(&self) -> Value {
        json!({
            "format_version": ERROR_FORMAT_VERSION,
            "error": { "code": self.code(), "message": self.message, "exit_code": self.exit_code },
        })
    }
}

impl From<StorageError> for ToolError {
    fn from(error: StorageError) -> Self {
        Self::new(error.exit_code(), error)
    }
}

impl From<DomainError> for ToolError {
    fn from(error: DomainError) -> Self {
        Self::new(3, error)
    }
}

impl From<IdError> for ToolError {
    fn from(error: IdError) -> Self {
        DomainError::InvalidId(error).into()
    }
}

impl From<WorkspaceSyncError> for ToolError {
    fn from(error: WorkspaceSyncError) -> Self {
        Self::new(error.exit_code(), error)
    }
}

impl From<VcsError> for ToolError {
    fn from(error: VcsError) -> Self {
        let exit_code = match error {
            VcsError::Discovery { .. } | VcsError::BareRepository { .. } | VcsError::MissingWorktree { .. } => 3,
            VcsError::PathOutsideWorktree { .. } | VcsError::Operation { .. } => 1,
        };
        Self::new(exit_code, error)
    }
}

impl IntoCallToolResult for ToolError {
    fn into_call_tool_result(self) -> Result<CallToolResponse, rmcp::ErrorData> {
        Ok(CallToolResult::structured_error(self.envelope()).into())
    }
}
