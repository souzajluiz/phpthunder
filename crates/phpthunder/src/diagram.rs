//! UML class diagrams: the classes around one class (its supertypes and
//! subtypes) or a chosen set of classes, with their declared members and
//! the extends / implements / uses relations between them, laid out in
//! layers (supertypes above) and drawn as SVG or written as PlantUML.

use crate::index::Index;
use crate::refactor::subtypes;
use crate::symbol::{Kind, Symbol, Visibility};
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

/// Most members listed in one box; the rest are counted in a last line.
const MAX_MEMBERS: usize = 14;
/// Width of one character of the monospace font at [`FONT_SIZE`].
const CHAR_WIDTH: f32 = 7.3;
/// Font size of names and members.
const FONT_SIZE: f32 = 12.0;
/// Height of one text line in a box.
const LINE: f32 = 17.0;
/// Inner horizontal padding of a box.
const PAD: f32 = 12.0;
/// Space between two boxes of one layer.
const GAP_X: f32 = 36.0;
/// Space between two layers, where the relation lines run.
const GAP_Y: f32 = 64.0;
/// Margin around the whole drawing.
const MARGIN: f32 = 24.0;

/// How one class relates to a supertype.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    /// `extends`: a class's parent, or an interface's parent interface.
    Extends,
    /// `implements`: a class or enum implementing an interface.
    Implements,
    /// `use`: a class using a trait (also `@mixin`).
    Uses,
}

/// One member line of a class box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// `+`, `#` or `-`.
    pub visibility: char,
    /// Declared `static` (underlined in UML).
    pub is_static: bool,
    /// Declared `abstract` (italic in UML).
    pub is_abstract: bool,
    /// Text after the visibility, e.g. `save(user): bool` or `name: string`.
    pub text: String,
}

/// One class, interface, trait or enum of a diagram.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// Fully qualified name.
    pub fqn: String,
    /// What was declared; `None` when the class is not in the index.
    pub kind: Option<Kind>,
    /// Declared `abstract`.
    pub is_abstract: bool,
    /// Declared `final`.
    pub is_final: bool,
    /// Constants, properties, then methods, in declaration order.
    pub members: Vec<Member>,
    /// The class the diagram was asked for, drawn highlighted.
    pub focus: bool,
}

/// A relation from `from` (the subtype) to `to` (the supertype), as
/// indices into [`Diagram::nodes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    /// Subtype.
    pub from: usize,
    /// Supertype.
    pub to: usize,
    /// Kind of relation.
    pub relation: Relation,
}

/// Classes and the relations between them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diagram {
    /// Classes, in the order they were found.
    pub nodes: Vec<Node>,
    /// Relations among `nodes`.
    pub edges: Vec<Edge>,
}

/// Colors and font of the SVG drawing, so it matches the editor theme.
#[derive(Debug, Clone)]
pub struct Style {
    /// Page background.
    pub background: String,
    /// Box fill.
    pub fill: String,
    /// Box fill of the focused class.
    pub focus_fill: String,
    /// Box border and relation lines.
    pub line: String,
    /// Class names and members.
    pub text: String,
    /// Stereotypes, namespaces and the "more" line.
    pub muted: String,
    /// Border of the focused class.
    pub accent: String,
    /// Font family list.
    pub font: String,
}

impl Default for Style {
    /// Light colors on white, for exports read outside the editor.
    fn default() -> Self {
        Self {
            background: "#ffffff".into(),
            fill: "#f7f7fa".into(),
            focus_fill: "#fff3f0".into(),
            line: "#55556a".into(),
            text: "#1d1d26".into(),
            muted: "#777790".into(),
            accent: "#fd1b1c".into(),
            font: "Menlo, Consolas, 'DejaVu Sans Mono', monospace".into(),
        }
    }
}

/// The diagram of `class`: the class itself, every supertype (parents,
/// interfaces, traits) and its subtypes, with their relations.
pub fn hierarchy_diagram(index: &Index, class: &str) -> Diagram {
    let mut names = index.hierarchy(class);
    names.extend(subtypes(index, class).into_iter().map(|(_, s)| s.fqn));
    let mut diagram = classes_diagram(index, &names);
    for node in &mut diagram.nodes {
        node.focus = node.fqn.eq_ignore_ascii_case(class);
    }
    diagram
}

/// The diagram of `classes` (fully qualified names) and the relations among
/// them; supertypes outside the set are left out. Duplicates are dropped.
pub fn classes_diagram(index: &Index, classes: &[String]) -> Diagram {
    let mut diagram = Diagram::default();
    let mut ids: HashMap<String, usize> = HashMap::new();
    let mut decls: Vec<Option<Symbol>> = Vec::new();
    for fqn in classes {
        let key = fqn.trim_start_matches('\\').to_ascii_lowercase();
        if ids.contains_key(&key) {
            continue;
        }
        ids.insert(key, diagram.nodes.len());
        let found = index.classes(fqn).into_iter().next();
        let members = found
            .map(|(path, decl)| {
                index
                    .file_symbols(path)
                    .iter()
                    .filter(|s| s.kind.is_member() && s.fqn.eq_ignore_ascii_case(&decl.fqn))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let decl = found.map(|(_, s)| s.clone());
        diagram.nodes.push(Node {
            fqn: decl.as_ref().map_or_else(|| fqn.clone(), |d| d.fqn.clone()),
            kind: decl.as_ref().map(|d| d.kind),
            is_abstract: decl.as_ref().is_some_and(|d| d.is_abstract),
            is_final: decl.as_ref().is_some_and(|d| d.is_final),
            members: member_lines(&members),
            focus: false,
        });
        decls.push(decl);
    }
    for (from, decl) in decls.iter().enumerate() {
        let Some(decl) = decl else {
            continue;
        };
        for sup in &decl.supers {
            let Some(&to) = ids.get(&sup.trim_start_matches('\\').to_ascii_lowercase()) else {
                continue;
            };
            if to == from {
                continue;
            }
            let relation = relation(decl, sup, diagram.nodes[to].kind);
            diagram.edges.push(Edge { from, to, relation });
        }
    }
    diagram
}

/// How `decl` relates to its supertype `sup`, whose kind is `sup_kind`
/// when indexed.
fn relation(decl: &Symbol, sup: &str, sup_kind: Option<Kind>) -> Relation {
    let is_parent = decl
        .extends
        .as_deref()
        .is_some_and(|e| e.eq_ignore_ascii_case(sup));
    match (decl.kind, sup_kind) {
        (_, Some(Kind::Trait)) => Relation::Uses,
        (Kind::Interface, _) => Relation::Extends,
        _ if is_parent => Relation::Extends,
        (_, Some(Kind::Interface)) => Relation::Implements,
        (_, Some(Kind::Class)) => Relation::Uses,
        _ => Relation::Implements,
    }
}

/// UML lines for a class's own members: constants and cases, properties,
/// then methods; each group in declaration order.
fn member_lines(members: &[&Symbol]) -> Vec<Member> {
    let order = |k: Kind| match k {
        Kind::ClassConstant | Kind::EnumCase => 0,
        Kind::Property => 1,
        _ => 2,
    };
    let mut sorted: Vec<&&Symbol> = members.iter().filter(|s| !s.is_virtual).collect();
    sorted.sort_by_key(|s| (order(s.kind), s.name_start));
    sorted
        .into_iter()
        .map(|s| Member {
            visibility: match s.visibility {
                Visibility::Public => '+',
                Visibility::Protected => '#',
                Visibility::Private => '-',
            },
            is_static: s.is_static && matches!(s.kind, Kind::Method | Kind::Property),
            is_abstract: s.is_abstract,
            text: member_text(s),
        })
        .collect()
}

/// `save(user, force): bool`, `name: string`, `MAX`.
fn member_text(s: &Symbol) -> String {
    match s.kind {
        Kind::Method => {
            let params: Vec<&str> = s.params.iter().map(|p| p.name.as_str()).collect();
            let ret = s
                .signature
                .rsplit_once("):")
                .map(|(_, r)| r.trim())
                .filter(|r| !r.is_empty())
                .map(|r| format!(": {r}"))
                .unwrap_or_default();
            format!("{}({}){ret}", s.name, params.join(", "))
        }
        Kind::Property => {
            let ty = s
                .signature
                .split_once('$')
                .map(|(head, _)| head)
                .and_then(|head| head.split_whitespace().last())
                .filter(|t| {
                    !matches!(
                        *t,
                        "public" | "protected" | "private" | "static" | "readonly" | "var"
                    )
                });
            match ty {
                Some(ty) => format!("{}: {ty}", s.name),
                None => s.name.clone(),
            }
        }
        _ => s.name.clone(),
    }
}

/// `«interface»`, `«trait»`, `«enum»`, `«abstract»`, or nothing.
fn stereotype(node: &Node) -> Option<&'static str> {
    match node.kind {
        Some(Kind::Interface) => Some("«interface»"),
        Some(Kind::Trait) => Some("«trait»"),
        Some(Kind::Enum) => Some("«enum»"),
        _ if node.is_abstract => Some("«abstract»"),
        _ if node.is_final => Some("«final»"),
        _ => None,
    }
}

/// Short class name: the part after the last backslash.
fn short_name(fqn: &str) -> &str {
    fqn.rsplit('\\').next().unwrap_or(fqn)
}

/// Position and size of a box.
#[derive(Debug, Clone, Copy, Default)]
struct Rect {
    /// Left edge.
    x: f32,
    /// Top edge.
    y: f32,
    /// Width.
    w: f32,
    /// Height.
    h: f32,
}

/// Layer of each node: 0 for nodes without supertypes in the diagram, else
/// one below their lowest supertype. Cycles are cut.
fn layers(diagram: &Diagram) -> Vec<usize> {
    let n = diagram.nodes.len();
    let mut parents: Vec<Vec<usize>> = vec![Vec::new(); n];
    for e in &diagram.edges {
        parents[e.from].push(e.to);
    }
    let mut layer: Vec<Option<usize>> = vec![None; n];
    /// Depth-first layer of `i`; `visiting` cuts cycles.
    fn visit(
        i: usize,
        parents: &[Vec<usize>],
        layer: &mut [Option<usize>],
        visiting: &mut HashSet<usize>,
    ) -> usize {
        if let Some(l) = layer[i] {
            return l;
        }
        if !visiting.insert(i) {
            return 0;
        }
        let l = parents[i]
            .iter()
            .map(|&p| visit(p, parents, layer, visiting) + 1)
            .max()
            .unwrap_or(0);
        visiting.remove(&i);
        layer[i] = Some(l);
        l
    }
    let mut visiting = HashSet::new();
    (0..n)
        .map(|i| visit(i, &parents, &mut layer, &mut visiting))
        .collect()
}

/// Box size for a node: wide enough for its longest line, tall enough for
/// its name, stereotype and (capped) members.
fn box_size(node: &Node) -> (f32, f32) {
    let shown = node.members.len().min(MAX_MEMBERS);
    let more = usize::from(node.members.len() > MAX_MEMBERS);
    let longest = node
        .members
        .iter()
        .take(MAX_MEMBERS)
        .map(|m| m.text.chars().count() + 2)
        .chain([short_name(&node.fqn).chars().count() + 2])
        .chain(stereotype(node).map(|s| s.chars().count()))
        .max()
        .unwrap_or(8);
    let header = if stereotype(node).is_some() { 2.0 } else { 1.0 };
    let body = (shown + more).max(1) as f32;
    let w = (longest as f32 * CHAR_WIDTH + 2.0 * PAD).max(120.0);
    let h = (header + body) * LINE + 18.0;
    (w, h)
}

/// Places every node: layers top to bottom, each layer ordered by the
/// average position of its supertypes, centered on the widest layer.
fn layout(diagram: &Diagram) -> (Vec<Rect>, f32, f32) {
    let layer = layers(diagram);
    let depth = layer.iter().copied().max().map_or(0, |m| m + 1);
    let mut rects: Vec<Rect> = diagram
        .nodes
        .iter()
        .map(|n| {
            let (w, h) = box_size(n);
            Rect {
                w,
                h,
                ..Rect::default()
            }
        })
        .collect();
    let mut rows: Vec<Vec<usize>> = vec![Vec::new(); depth];
    for (i, &l) in layer.iter().enumerate() {
        rows[l].push(i);
    }
    let mut y = MARGIN;
    let mut widths = Vec::with_capacity(depth);
    for row in &mut rows {
        let center = |i: usize, rects: &[Rect]| {
            let xs: Vec<f32> = diagram
                .edges
                .iter()
                .filter(|e| e.from == i)
                .map(|e| rects[e.to].x + rects[e.to].w / 2.0)
                .collect();
            if xs.is_empty() {
                f32::MAX
            } else {
                xs.iter().sum::<f32>() / xs.len() as f32
            }
        };
        let keys: HashMap<usize, f32> = row.iter().map(|&i| (i, center(i, &rects))).collect();
        row.sort_by(|a, b| {
            keys[a]
                .total_cmp(&keys[b])
                .then_with(|| diagram.nodes[*a].fqn.cmp(&diagram.nodes[*b].fqn))
        });
        let mut x = MARGIN;
        let mut tallest: f32 = 0.0;
        for &i in row.iter() {
            rects[i].x = x;
            rects[i].y = y;
            x += rects[i].w + GAP_X;
            tallest = tallest.max(rects[i].h);
        }
        widths.push(x - GAP_X + MARGIN);
        y += tallest + GAP_Y;
    }
    let width = widths.iter().copied().fold(2.0 * MARGIN, f32::max);
    for (row, w) in rows.iter().zip(&widths) {
        let shift = (width - w) / 2.0;
        for &i in row {
            rects[i].x += shift;
        }
    }
    let height = (y - GAP_Y + MARGIN).max(2.0 * MARGIN);
    (rects, width, height)
}

/// Escapes text for an SVG text node or attribute.
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Draws `diagram` as a standalone SVG document in `style`'s colors.
pub fn to_svg(diagram: &Diagram, style: &Style) -> String {
    let (rects, width, height) = layout(diagram);
    let mut out = String::new();
    let _ = write!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width:.0}" height="{height:.0}" viewBox="0 0 {width:.0} {height:.0}" font-family="{font}" font-size="{FONT_SIZE}">"#,
        font = escape(&style.font)
    );
    let _ = write!(
        out,
        r#"<rect width="100%" height="100%" fill="{}"/>"#,
        style.background
    );
    for e in &diagram.edges {
        edge_svg(&mut out, rects[e.from], rects[e.to], e.relation, style);
    }
    for (node, rect) in diagram.nodes.iter().zip(&rects) {
        node_svg(&mut out, node, *rect, style);
    }
    out.push_str("</svg>\n");
    out
}

/// Draws one relation: an elbow line from the top of the subtype to the
/// bottom of the supertype, ending in a hollow triangle (extends,
/// implements) or an open arrow (uses); implements and uses are dashed.
fn edge_svg(out: &mut String, from: Rect, to: Rect, relation: Relation, style: &Style) {
    let (x1, y1) = (from.x + from.w / 2.0, from.y);
    let (x2, y2) = (to.x + to.w / 2.0, to.y + to.h);
    let mid = y2 + (GAP_Y / 2.0).min((y1 - y2) / 2.0).max(8.0);
    let dash = match relation {
        Relation::Extends => "",
        Relation::Implements | Relation::Uses => r#" stroke-dasharray="5 4""#,
    };
    let tip = y2 + 10.0;
    let _ = write!(
        out,
        r#"<polyline points="{x1:.1},{y1:.1} {x1:.1},{mid:.1} {x2:.1},{mid:.1} {x2:.1},{tip:.1}" fill="none" stroke="{}" stroke-width="1.2"{dash}/>"#,
        style.line
    );
    match relation {
        Relation::Extends | Relation::Implements => {
            let _ = write!(
                out,
                r#"<polygon points="{x2:.1},{y2:.1} {:.1},{tip:.1} {:.1},{tip:.1}" fill="{}" stroke="{}" stroke-width="1.2"/>"#,
                x2 - 7.0,
                x2 + 7.0,
                style.background,
                style.line
            );
        }
        Relation::Uses => {
            let _ = write!(
                out,
                r#"<polyline points="{:.1},{tip:.1} {x2:.1},{y2:.1} {:.1},{tip:.1}" fill="none" stroke="{}" stroke-width="1.2"/>"#,
                x2 - 6.0,
                x2 + 6.0,
                style.line
            );
        }
    }
}

/// Draws one class box: stereotype, name (italic when abstract), a
/// separator, then its members (underlined when static, italic when
/// abstract).
fn node_svg(out: &mut String, node: &Node, r: Rect, style: &Style) {
    let (fill, stroke, width) = if node.focus {
        (&style.focus_fill, &style.accent, 2.0)
    } else {
        (&style.fill, &style.line, 1.0)
    };
    let _ = write!(
        out,
        r#"<g><title>{}</title><rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="5" fill="{fill}" stroke="{stroke}" stroke-width="{width}"/>"#,
        escape(&node.fqn),
        r.x,
        r.y,
        r.w,
        r.h
    );
    let cx = r.x + r.w / 2.0;
    let mut y = r.y + LINE;
    if let Some(s) = stereotype(node) {
        let _ = write!(
            out,
            r#"<text x="{cx:.1}" y="{y:.1}" text-anchor="middle" fill="{}" font-size="11">{s}</text>"#,
            style.muted
        );
        y += LINE;
    }
    let italic = if node.is_abstract || node.kind == Some(Kind::Interface) {
        r#" font-style="italic""#
    } else {
        ""
    };
    let _ = write!(
        out,
        r#"<text x="{cx:.1}" y="{y:.1}" text-anchor="middle" fill="{}" font-weight="bold"{italic}>{}</text>"#,
        style.text,
        escape(short_name(&node.fqn))
    );
    y += 8.0;
    let _ = write!(
        out,
        r#"<line x1="{:.1}" y1="{y:.1}" x2="{:.1}" y2="{y:.1}" stroke="{stroke}" stroke-width="1"/>"#,
        r.x,
        r.x + r.w
    );
    for m in node.members.iter().take(MAX_MEMBERS) {
        y += LINE;
        let mut attrs = String::new();
        if m.is_static {
            attrs.push_str(r#" text-decoration="underline""#);
        }
        if m.is_abstract {
            attrs.push_str(r#" font-style="italic""#);
        }
        let _ = write!(
            out,
            r#"<text x="{:.1}" y="{y:.1}" fill="{}"{attrs}><tspan fill="{}">{} </tspan>{}</text>"#,
            r.x + PAD,
            style.text,
            style.muted,
            m.visibility,
            escape(&m.text)
        );
    }
    if node.members.len() > MAX_MEMBERS {
        y += LINE;
        let _ = write!(
            out,
            r#"<text x="{:.1}" y="{y:.1}" fill="{}">… +{}</text>"#,
            r.x + PAD,
            style.muted,
            node.members.len() - MAX_MEMBERS
        );
    }
    out.push_str("</g>");
}

/// Writes `diagram` as PlantUML, for pasting into documentation.
pub fn to_plantuml(diagram: &Diagram) -> String {
    let mut out = String::from("@startuml\n");
    let alias = |fqn: &str| fqn.replace('\\', ".");
    for node in &diagram.nodes {
        let keyword = match node.kind {
            Some(Kind::Interface) => "interface",
            Some(Kind::Trait) => "class",
            Some(Kind::Enum) => "enum",
            _ if node.is_abstract => "abstract class",
            _ => "class",
        };
        let trait_mark = if node.kind == Some(Kind::Trait) {
            " <<trait>>"
        } else {
            ""
        };
        let _ = writeln!(out, "{keyword} {}{trait_mark} {{", alias(&node.fqn));
        for m in &node.members {
            let modifier = match (m.is_static, m.is_abstract) {
                (true, _) => "{static} ",
                (_, true) => "{abstract} ",
                _ => "",
            };
            let _ = writeln!(out, "  {modifier}{}{}", m.visibility, m.text);
        }
        out.push_str("}\n");
    }
    for e in &diagram.edges {
        let arrow = match e.relation {
            Relation::Extends => "<|--",
            Relation::Implements => "<|..",
            Relation::Uses => "<..",
        };
        let _ = writeln!(
            out,
            "{} {arrow} {}",
            alias(&diagram.nodes[e.to].fqn),
            alias(&diagram.nodes[e.from].fqn)
        );
    }
    out.push_str("@enduml\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::index_source;
    use std::path::PathBuf;

    const SRC: &str = r#"<?php
namespace App;
interface Shape { public function area(): float; }
trait Named { protected string $name; }
abstract class Base implements Shape {
    const SIDES = 0;
    public static function make(int $size): static {}
}
final class Square extends Base {
    use Named;
    private float $side;
    public function area(): float {}
}
"#;

    /// An index holding [`SRC`].
    fn index() -> Index {
        let mut index = Index::default();
        index.update(PathBuf::from("/app/a.php"), index_source(SRC));
        index
    }

    /// The edge between two named nodes, if any.
    fn edge(d: &Diagram, from: &str, to: &str) -> Option<Relation> {
        let pos = |n: &str| d.nodes.iter().position(|x| x.fqn.ends_with(n));
        let (f, t) = (pos(from)?, pos(to)?);
        d.edges
            .iter()
            .find(|e| e.from == f && e.to == t)
            .map(|e| e.relation)
    }

    #[test]
    fn hierarchy_diagram_should_hold_supertypes_and_subtypes() {
        let d = hierarchy_diagram(&index(), "App\\Base");
        let mut names: Vec<&str> = d.nodes.iter().map(|n| short_name(&n.fqn)).collect();
        names.sort_unstable();
        assert_eq!(names, ["Base", "Shape", "Square"]);
    }

    #[test]
    fn relations_should_follow_the_kind_of_supertype() {
        let d = classes_diagram(
            &index(),
            &["App\\Square", "App\\Base", "App\\Shape", "App\\Named"].map(String::from),
        );
        assert_eq!(
            [
                edge(&d, "Square", "Base"),
                edge(&d, "Base", "Shape"),
                edge(&d, "Square", "Named")
            ],
            [
                Some(Relation::Extends),
                Some(Relation::Implements),
                Some(Relation::Uses)
            ]
        );
    }

    #[test]
    fn members_should_list_constants_properties_then_methods() {
        let d = classes_diagram(&index(), &["App\\Base".to_string()]);
        let lines: Vec<String> = d.nodes[0]
            .members
            .iter()
            .map(|m| format!("{}{}", m.visibility, m.text))
            .collect();
        assert_eq!(lines, ["+SIDES", "+make(size): static"]);
    }

    #[test]
    fn property_types_should_come_from_the_signature() {
        let d = classes_diagram(&index(), &["App\\Square".to_string()]);
        assert_eq!(d.nodes[0].members[0].text, "side: float");
    }

    #[test]
    fn supertypes_should_be_drawn_above_subtypes() {
        let d = hierarchy_diagram(&index(), "App\\Square");
        let (rects, _, _) = layout(&d);
        let y = |n: &str| rects[d.nodes.iter().position(|x| x.fqn.ends_with(n)).unwrap()].y;
        assert!(y("Shape") < y("Base") && y("Base") < y("Square"));
    }

    #[test]
    fn svg_should_escape_names_and_mark_the_focus() {
        let svg = to_svg(
            &hierarchy_diagram(&index(), "App\\Square"),
            &Style::default(),
        );
        assert!(svg.starts_with("<svg") && svg.contains("«interface»") && svg.contains("#fd1b1c"));
    }

    #[test]
    fn plantuml_should_draw_each_relation() {
        let uml = to_plantuml(&hierarchy_diagram(&index(), "App\\Square"));
        assert!(
            uml.contains("App.Base <|-- App.Square") && uml.contains("App.Shape <|.. App.Base")
        );
    }
}
