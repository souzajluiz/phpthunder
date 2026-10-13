//! `phpthunder` binary: serves the language server over stdin/stdout.
//!
//! `--version` prints the version and exits; any other invocation speaks LSP.

use tower_lsp::{LspService, Server};

/// Runs the LSP server on stdin/stdout until the client disconnects.
#[tokio::main]
async fn main() {
    if std::env::args().any(|a| a == "--version") {
        println!("phpthunder {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let (service, socket) = LspService::new(phpthunder::server::Backend::new);
    Server::new(tokio::io::stdin(), tokio::io::stdout(), socket)
        .serve(service)
        .await;
}
