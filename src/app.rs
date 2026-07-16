use std::io::{self, Write};

use anyhow::{Context, Result};

use crate::cli::Cli;
use crate::output::{OutputMode, Renderer};

/// Run one parsed CLI invocation at the application boundary.
pub fn run(cli: Cli) -> Result<()> {
    let mode = if cli.json {
        OutputMode::Json
    } else if cli.plain {
        OutputMode::Plain
    } else if cli.quiet {
        OutputMode::Quiet
    } else {
        OutputMode::Human
    };

    let renderer = Renderer::new(mode, cli.color);
    let Some(message) = renderer.render_startup().context("rendering CLI output")? else {
        return Ok(());
    };

    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    writeln!(stdout, "{message}").context("writing CLI output")?;
    Ok(())
}
