use std::path::Path;

use owo_colors::{OwoColorize, Stream};
use serde::Serialize;
use thiserror::Error;

use crate::cli::ColorChoice;

/// The output format for a command result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputMode {
    Human,
    Plain,
    Json,
    Quiet,
}

/// Output failures that can occur before the application boundary adds context.
#[derive(Debug, Error)]
pub enum OutputError {
    #[error("could not serialize JSON output: {0}")]
    Json(#[from] serde_json::Error),
}

/// The renderer used by the application boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Renderer {
    mode: OutputMode,
    color: ColorChoice,
}

impl Renderer {
    pub const fn new(mode: OutputMode, color: ColorChoice) -> Self {
        Self { mode, color }
    }

    /// Render the result of starting the foundation-only application.
    pub fn render_startup(&self) -> Result<Option<String>, OutputError> {
        let message = match self.mode {
            OutputMode::Human => self.human_message(),
            OutputMode::Plain => "ready".to_owned(),
            OutputMode::Json => serde_json::to_string(&Envelope {
                format_version: 1,
                data: StartupData { status: "ready", message: "Arc Lightning foundation ready" },
            })?,
            OutputMode::Quiet => return Ok(None),
        };

        Ok(Some(message))
    }

    /// Render the result of project initialization.
    pub fn render_init(&self, root: &Path, snapshot_enabled: bool) -> Result<Option<String>, OutputError> {
        let root = root.to_string_lossy();
        let message = match self.mode {
            OutputMode::Human => {
                let snapshot = if snapshot_enabled { " with snapshots enabled" } else { "" };
                format!("Initialized Arc Lightning in `{root}`{snapshot}")
            }
            OutputMode::Plain => format!("initialized\t{root}"),
            OutputMode::Json => serde_json::to_string(&Envelope {
                format_version: 1,
                data: InitData { status: "initialized", root: root.as_ref(), snapshot_enabled },
            })?,
            OutputMode::Quiet => return Ok(None),
        };

        Ok(Some(match (self.mode, self.color) {
            (OutputMode::Human, ColorChoice::Always) => message.bright_blue().to_string(),
            (OutputMode::Human, ColorChoice::Auto) => message
                .if_supports_color(Stream::Stdout, |text| text.bright_blue())
                .to_string(),
            _ => message,
        }))
    }

    fn human_message(&self) -> String {
        const MESSAGE: &str = "Arc Lightning foundation ready";
        match self.color {
            ColorChoice::Always => MESSAGE.bright_blue().to_string(),
            ColorChoice::Never => MESSAGE.to_owned(),
            ColorChoice::Auto => MESSAGE
                .if_supports_color(Stream::Stdout, |text| text.bright_blue())
                .to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
struct Envelope<T> {
    format_version: u8,
    data: T,
}

#[derive(Debug, Serialize)]
struct StartupData {
    status: &'static str,
    message: &'static str,
}

#[derive(Debug, Serialize)]
struct InitData<'a> {
    status: &'static str,
    root: &'a str,
    snapshot_enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::ColorChoice;

    #[test]
    fn plain_output_is_stable_and_unstyled() {
        let rendered = Renderer::new(OutputMode::Plain, ColorChoice::Always)
            .render_startup()
            .expect("rendering succeeds")
            .expect("plain output is not quiet");

        assert_eq!(rendered, "ready");
        assert!(!rendered.contains('\u{1b}'));
    }

    #[test]
    fn quiet_output_is_empty() {
        assert_eq!(
            Renderer::new(OutputMode::Quiet, ColorChoice::Always)
                .render_startup()
                .expect("rendering succeeds"),
            None
        );
    }
}
