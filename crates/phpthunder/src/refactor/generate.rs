//! Code generation inside a class (`⌘N` in the IDE): constructor, getters
//! and setters, and overrides of parent methods.

use super::*;
use crate::index::Index;
use crate::symbol::{Kind, Symbol};
use crate::syntax::{context_at, is_class_like, parse, text};
use tree_sitter::Node;

/// Outline of the class-like declaration around byte `offset` of `src`;
/// `None` outside a class.
pub fn class_outline(src: &str, offset: usize) -> Option<ClassOutline> {
    let tree = parse(src);
    let mut node = tree.root_node().descendant_for_byte_range(offset, offset)?;
    while !is_class_like(node.kind()) {
        node = node.parent()?;
    }
    let name = node.child_by_field_name("name")?;
    let fqn = context_at(tree.root_node(), src, offset).qualify(text(name, src));
    let body = node.child_by_field_name("body")?;
    let mut outline = ClassOutline {
        fqn,
        body_end: body.end_byte().checked_sub(1)?,
        properties: Vec::new(),
        methods: Vec::new(),
    };
    let mut cursor = body.walk();
    for member in body.named_children(&mut cursor) {
        match member.kind() {
            "property_declaration" => property_declaration(member, src, &mut outline.properties),
            "method_declaration" => {
                if let Some(name) = member.child_by_field_name("name") {
                    let name = text(name, src);
                    outline.methods.push(name.to_ascii_lowercase());
                    if name.eq_ignore_ascii_case("__construct") {
                        promoted_properties(member, src, &mut outline.properties);
                    }
                }
            }
            _ => {}
        }
    }
    Some(outline)
}

/// Adds the properties of one `property_declaration` (`private int $a, $b = 1;`).
fn property_declaration(decl: Node, src: &str, out: &mut Vec<ClassProperty>) {
    let mut cursor = decl.walk();
    let children: Vec<Node> = decl.named_children(&mut cursor).collect();
    let is_static = children.iter().any(|c| c.kind() == "static_modifier");
    let is_readonly = children.iter().any(|c| c.kind() == "readonly_modifier");
    let ty = decl
        .child_by_field_name("type")
        .map(|t| text(t, src).to_string());
    for element in children.iter().filter(|c| c.kind() == "property_element") {
        let Some(name) = element.child_by_field_name("name") else {
            continue;
        };
        out.push(ClassProperty {
            name: text(name, src).trim_start_matches('$').to_string(),
            ty: ty.clone(),
            is_static,
            is_readonly,
            is_initialized: element.child_by_field_name("default_value").is_some(),
        });
    }
}

/// Adds constructor-promoted properties (`public function __construct(private int $a)`).
fn promoted_properties(method: Node, src: &str, out: &mut Vec<ClassProperty>) {
    let Some(params) = method.child_by_field_name("parameters") else {
        return;
    };
    let mut cursor = params.walk();
    for param in params
        .named_children(&mut cursor)
        .filter(|p| p.kind() == "property_promotion_parameter")
    {
        let Some(name) = param.child_by_field_name("name") else {
            continue;
        };
        let mut inner = param.walk();
        let is_readonly = param
            .named_children(&mut inner)
            .any(|c| c.kind() == "readonly_modifier");
        out.push(ClassProperty {
            name: text(name, src).trim_start_matches('$').to_string(),
            ty: param
                .child_by_field_name("type")
                .map(|t| text(t, src).to_string()),
            is_static: false,
            is_readonly,
            is_initialized: true,
        });
    }
}

/// `name` with its first letter uppercased (`firstName` -> `FirstName`).
fn capitalize(name: &str) -> String {
    let mut chars = name.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// A typed parameter or return type prefix: `"int $a"` or `"$a"`.
fn typed(ty: Option<&str>, rest: &str) -> String {
    match ty {
        Some(ty) => format!("{ty} {rest}"),
        None => rest.to_string(),
    }
}

/// A constructor taking and assigning every instance property that has
/// no default value. `None` when the class already has a constructor or
/// no property needs one.
pub fn constructor_source(outline: &ClassOutline) -> Option<String> {
    if outline.declares("__construct") {
        return None;
    }
    let props: Vec<&ClassProperty> = outline
        .properties
        .iter()
        .filter(|p| !p.is_static && !p.is_initialized)
        .collect();
    if props.is_empty() {
        return None;
    }
    let params: Vec<String> = props
        .iter()
        .map(|p| typed(p.ty.as_deref(), &format!("${}", p.name)))
        .collect();
    let body: String = props
        .iter()
        .map(|p| format!("        $this->{0} = ${0};\n", p.name))
        .collect();
    Some(format!(
        "\n    public function __construct({})\n    {{\n{body}    }}\n",
        params.join(", ")
    ))
}

/// Name of the getter for `property`: `isActive` for booleans, else `getName`.
pub fn getter_name(property: &ClassProperty) -> String {
    let is_bool = matches!(property.ty.as_deref(), Some("bool" | "?bool"));
    let prefix = if is_bool { "is" } else { "get" };
    format!("{prefix}{}", capitalize(&property.name))
}

/// Name of the setter for `property` (`setName`).
pub fn setter_name(property: &ClassProperty) -> String {
    format!("set{}", capitalize(&property.name))
}

/// Getters and/or setters for every instance property that lacks them
/// (readonly properties get no setter). `None` when nothing is missing.
pub fn accessors_source(outline: &ClassOutline, getters: bool, setters: bool) -> Option<String> {
    let mut out = String::new();
    for p in outline.properties.iter().filter(|p| !p.is_static) {
        let getter = getter_name(p);
        if getters && !outline.declares(&getter) {
            let ret =
                p.ty.as_deref()
                    .map(|t| format!(": {t}"))
                    .unwrap_or_default();
            out.push_str(&format!(
                "\n    public function {getter}(){ret}\n    {{\n        return $this->{};\n    }}\n",
                p.name
            ));
        }
        let setter = setter_name(p);
        if setters && !p.is_readonly && !outline.declares(&setter) {
            let param = typed(p.ty.as_deref(), &format!("${}", p.name));
            out.push_str(&format!(
                "\n    public function {setter}({param}): static\n    {{\n        $this->{0} = ${0};\n\n        return $this;\n    }}\n",
                p.name
            ));
        }
    }
    (!out.is_empty()).then_some(out)
}

/// Whether the class is a PHPUnit (or Laravel, Pest-compatible) test
/// case: its hierarchy reaches a class named `TestCase`.
pub fn is_test_class(index: &Index, outline: &ClassOutline) -> bool {
    index.hierarchy(&outline.fqn).iter().skip(1).any(|c| {
        c.rsplit('\\')
            .next()
            .is_some_and(|s| s.eq_ignore_ascii_case("TestCase"))
    })
}

/// An empty test method, named `testExample` (numbered when taken), for
/// the Generate menu of a test class.
pub fn test_method_source(outline: &ClassOutline) -> String {
    let name = (1..)
        .map(|i| {
            if i == 1 {
                "testExample".to_string()
            } else {
                format!("testExample{i}")
            }
        })
        .find(|n| !outline.declares(n))
        .unwrap_or_default();
    format!("\n    public function {name}(): void\n    {{\n    }}\n")
}

/// A `setUp` or `tearDown` fixture calling its parent; `None` when the
/// class declares it already.
pub fn fixture_source(outline: &ClassOutline, name: &str) -> Option<String> {
    (!outline.declares(name)).then(|| {
        format!(
            "\n    protected function {name}(): void\n    {{\n        parent::{name}();\n    }}\n"
        )
    })
}

/// Parent-class methods the class can override: public or protected,
/// not final, not abstract (those are implemented, not overridden), and
/// not already declared. Nearest declaration wins; sorted by name.
pub fn overridable_methods(index: &Index, outline: &ClassOutline) -> Vec<Symbol> {
    let mut out: Vec<Symbol> = Vec::new();
    for cls in index.hierarchy(&outline.fqn).iter().skip(1) {
        let Some((path, decl)) = index.classes(cls).into_iter().next() else {
            continue;
        };
        if decl.kind == Kind::Interface {
            continue;
        }
        for method in index.file_symbols(path).iter().filter(|s| {
            s.kind == Kind::Method && !s.is_virtual && s.fqn.eq_ignore_ascii_case(&decl.fqn)
        }) {
            let words: Vec<&str> = method.signature.split_whitespace().collect();
            let blocked = words
                .iter()
                .any(|w| matches!(*w, "private" | "final" | "abstract"));
            if blocked
                || outline.declares(&method.name)
                || out
                    .iter()
                    .any(|m| m.name.eq_ignore_ascii_case(&method.name))
            {
                continue;
            }
            out.push(method.clone());
        }
    }
    out.sort_by_key(|m| m.name.to_ascii_lowercase());
    out
}

/// Parameter names of a signature, in order, with `...` for variadics
/// (`f(int $a, string ...$rest)` -> `["$a", "...$rest"]`).
fn argument_names(signature: &str) -> Vec<String> {
    let Some(open) = signature.find('(') else {
        return Vec::new();
    };
    let mut depth = 0usize;
    let mut close = signature.len();
    for (i, c) in signature[open..].char_indices() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => {
                depth -= 1;
                if depth == 0 {
                    close = open + i;
                    break;
                }
            }
            _ => {}
        }
    }
    let params = &signature[open + 1..close];
    let mut names = Vec::new();
    let bytes = params.as_bytes();
    let mut i = 0;
    let mut in_default = false;
    while i < bytes.len() {
        match bytes[i] {
            b',' => in_default = false,
            b'=' => in_default = true,
            b'$' if !in_default => {
                let end = params[i + 1..]
                    .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                    .map_or(params.len(), |e| i + 1 + e);
                let variadic = params[..i].trim_end().ends_with("...");
                let name = &params[i..end];
                names.push(if variadic {
                    format!("...{name}")
                } else {
                    name.to_string()
                });
                i = end;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    names
}

/// Source overriding `method`: same signature, body calling the parent.
pub fn override_stub(method: &Symbol) -> String {
    let header: Vec<&str> = method
        .signature
        .split_whitespace()
        .filter(|w| *w != "final")
        .collect();
    let header = header.join(" ");
    let call = format!(
        "parent::{}({})",
        method.name,
        argument_names(&method.signature).join(", ")
    );
    let returns_nothing = header.rsplit_once(')').is_some_and(|(_, ret)| {
        matches!(ret.trim().trim_start_matches(':').trim(), "void" | "never")
    }) || method.name.eq_ignore_ascii_case("__construct");
    let body = if returns_nothing {
        format!("{call};")
    } else {
        format!("return {call};")
    };
    format!("\n    {header}\n    {{\n        {body}\n    }}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLASS: &str = "<?php\nnamespace App;\nclass User extends Model {\n    private static int $count = 0;\n    private string $name;\n    private ?bool $active;\n    protected readonly int $id;\n    private array $tags = [];\n    public function getName(): string { return $this->name; }\n}\n";

    fn outline() -> ClassOutline {
        class_outline(CLASS, CLASS.find("private string").unwrap()).unwrap()
    }

    #[test]
    fn test_class_should_get_numbered_test_method_and_missing_fixtures() {
        let src = "<?php\nclass UserTest extends TestCase {\n    public function testExample(): void {}\n    protected function setUp(): void {}\n}\n";
        let mut index = Index::default();
        index.update(
            std::path::PathBuf::from("/t.php"),
            crate::index::index_source("<?php class TestCase {}"),
        );
        index.update(
            std::path::PathBuf::from("/u.php"),
            crate::index::index_source(src),
        );
        let o = class_outline(src, src.find("testExample").unwrap()).unwrap();
        assert!(is_test_class(&index, &o));
        assert!(test_method_source(&o).contains("function testExample2(): void"));
        assert_eq!(fixture_source(&o, "setUp"), None);
        assert!(fixture_source(&o, "tearDown").is_some_and(|s| s.contains("parent::tearDown();")));
    }

    #[test]
    fn class_outline_should_list_properties_and_methods() {
        let o = outline();
        assert_eq!(o.fqn, "App\\User");
        let names: Vec<&str> = o.properties.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["count", "name", "active", "id", "tags"]);
        assert_eq!(o.methods, ["getname"]);
        assert_eq!(&CLASS[o.body_end..o.body_end + 1], "}");
    }

    #[test]
    fn constructor_should_take_uninitialized_instance_properties() {
        let src = constructor_source(&outline()).unwrap();
        assert!(src.contains("__construct(string $name, ?bool $active, int $id)"));
        assert!(src.contains("$this->active = $active;"));
        assert!(!src.contains("tags"));
    }

    #[test]
    fn accessors_should_skip_existing_methods_and_readonly_setters() {
        let src = accessors_source(&outline(), true, true).unwrap();
        assert!(!src.contains("function getName"));
        assert!(src.contains("public function setName(string $name): static"));
        assert!(src.contains("public function isActive(): ?bool"));
        assert!(src.contains("public function getId(): int"));
        assert!(!src.contains("setId"));
        assert!(!src.contains("Count"));
    }

    #[test]
    fn argument_names_should_skip_defaults_and_keep_variadics() {
        assert_eq!(
            argument_names("public function f(int $a, string $b = '$x', ...$rest): void"),
            ["$a", "$b", "...$rest"]
        );
    }

    #[test]
    fn override_stub_should_call_parent() {
        let mut m = Symbol {
            kind: Kind::Method,
            name: "save".into(),
            fqn: "Model".into(),
            span: Default::default(),
            name_span: Default::default(),
            name_start: 0,
            is_static: false,
            visibility: Default::default(),
            signature: "public function save(array $options = []): bool".into(),
            doc: String::new(),
            ty: Default::default(),
            extends: None,
            supers: Vec::new(),
            is_virtual: false,
            templates: Vec::new(),
            super_args: Vec::new(),
            class_string_params: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly: false,
            params: Vec::new(),
            reads_extra_args: false,
            is_deprecated: false,
        };
        assert!(override_stub(&m).contains("return parent::save($options);"));
        m.signature = "protected function boot(): void".into();
        m.name = "boot".into();
        assert!(override_stub(&m).contains("        parent::boot();\n"));
    }
}
