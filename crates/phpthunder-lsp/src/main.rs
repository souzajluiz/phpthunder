//! `phpthunder` binary: serves the language server over stdin/stdout.
//!
//! `--version` prints the version and exits; `--mcp [project]` serves the
//! project index to AI agents over MCP; any other invocation speaks LSP.

mod convert;
mod mcp;
mod server;

use tower_lsp::{LspService, Server};

/// Runs the LSP server on stdin/stdout until the client disconnects.
#[tokio::main]
async fn main() {
    if std::env::args().any(|a| a == "--version") {
        println!("phpthunder {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let args: Vec<String> = std::env::args().collect();
    if let Some(at) = args.iter().position(|a| a == "--mcp") {
        let root = args
            .get(at + 1)
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        // The index is built and queried synchronously; keep it off the
        // async runtime's workers.
        let served = tokio::task::spawn_blocking(move || mcp::serve(root)).await;
        if let Ok(Err(error)) = served {
            eprintln!("phpthunder: {error}");
        }
        return;
    }
    let (service, socket) = LspService::new(server::Backend::new);
    Server::new(tokio::io::stdin(), tokio::io::stdout(), socket)
        .serve(service)
        .await;
}
