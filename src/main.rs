use tower_lsp::{LspService, Server};

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
