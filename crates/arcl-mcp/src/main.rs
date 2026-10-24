//! `arcl-mcp`: the Arc Lightning MCP server over stdio.
//!
//! An MCP client starts it in the project directory. It serves the project
//! that holds that directory until the client closes standard input.

use rmcp::{ServiceExt, transport::stdio};

use arcl_mcp::server::ArclServer;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(error) = serve().await {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

async fn serve() -> Result<(), Box<dyn std::error::Error>> {
    let start = std::env::current_dir()?;
    let service = ArclServer::new(start).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
