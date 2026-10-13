//! Prints the named nodes of a PHP file's syntax tree, one per line.
//!
//! Usage: `cargo run --example dump -- path/to/file.php`

fn walk(node: tree_sitter::Node, src: &[u8], depth: usize) {
    if node.is_named() {
        let leaf = if node.child_count() == 0 {
            node.utf8_text(src).unwrap_or("")
        } else {
            ""
        };
        println!("{}{} {leaf}", "  ".repeat(depth), node.kind());
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, src, depth + 1);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: dump <file.php>")?;
    let src = std::fs::read(path)?;
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_php::LANGUAGE_PHP.into())?;
    let tree = parser.parse(&src, None).ok_or("parse failed")?;
    walk(tree.root_node(), &src, 0);
    Ok(())
}
