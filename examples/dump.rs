fn walk(n: tree_sitter::Node, src: &[u8], depth: usize) {
    if n.is_named() {
        let t = if n.child_count() == 0 {
            n.utf8_text(src).unwrap_or("")
        } else {
            ""
        };
        let field = "";
        println!("{}{}{} {}", "  ".repeat(depth), n.kind(), field, t);
    }
    let mut c = n.walk();
    for (i, ch) in n.children(&mut c).enumerate() {
        let _ = i;
        walk(ch, src, depth + 1);
    }
}
fn main() {
    let src = std::fs::read(std::env::args().nth(1).unwrap()).unwrap();
    let mut p = tree_sitter::Parser::new();
    p.set_language(&tree_sitter_php::LANGUAGE_PHP.into())
        .unwrap();
    let t = p.parse(&src, None).unwrap();
    walk(t.root_node(), &src, 0);
}
