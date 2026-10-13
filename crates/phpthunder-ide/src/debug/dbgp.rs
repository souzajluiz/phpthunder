//! The DBGp wire protocol: framing, commands, and reading responses into
//! frames and variables.

use super::*;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use std::path::{Path, PathBuf};

/// A command line for Xdebug: `name -i tx args [-- base64(data)]` and NUL.
pub fn encode_command(name: &str, transaction: u32, args: &str, data: Option<&str>) -> Vec<u8> {
    let mut line = format!("{name} -i {transaction}");
    if !args.is_empty() {
        line.push(' ');
        line.push_str(args);
    }
    if let Some(data) = data {
        line.push_str(" -- ");
        line.push_str(&BASE64.encode(data));
    }
    let mut bytes = line.into_bytes();
    bytes.push(0);
    bytes
}

/// Splits Xdebug's `length NUL xml NUL` packets out of a byte stream.
#[derive(Debug, Default)]
pub struct PacketReader {
    /// Bytes received and not yet consumed.
    buffer: Vec<u8>,
}

impl PacketReader {
    /// Adds received bytes.
    pub fn push(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    /// The next complete packet's XML, if one has fully arrived.
    pub fn next_packet(&mut self) -> Option<String> {
        let nul = self.buffer.iter().position(|b| *b == 0)?;
        let len: usize = std::str::from_utf8(&self.buffer[..nul])
            .ok()?
            .trim()
            .parse()
            .ok()?;
        let end = nul + 1 + len;
        if self.buffer.len() < end + 1 {
            return None;
        }
        let xml = String::from_utf8_lossy(&self.buffer[nul + 1..end]).into_owned();
        self.buffer.drain(..=end);
        Some(xml)
    }
}

/// `file://` URI of a local path, as Xdebug expects (spaces and `%` escaped).
pub fn file_uri(path: &Path) -> String {
    let raw = path.to_string_lossy();
    let mut out = String::from("file://");
    for c in raw.chars() {
        match c {
            ' ' => out.push_str("%20"),
            '%' => out.push_str("%25"),
            '#' => out.push_str("%23"),
            '?' => out.push_str("%3F"),
            _ => out.push(c),
        }
    }
    out
}

/// Local path of a `file://` URI, percent escapes decoded.
pub fn uri_path(uri: &str) -> PathBuf {
    let raw = uri.strip_prefix("file://").unwrap_or(uri);
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(hex) = raw.get(i + 1..i + 3)
            && let Ok(b) = u8::from_str_radix(hex, 16)
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    PathBuf::from(String::from_utf8_lossy(&out).into_owned())
}

/// Error message of a response, when it reports one.
pub(crate) fn response_error(response: &Element) -> Option<String> {
    let error = response.children_named("error").next()?;
    let message = error
        .children_named("message")
        .next()
        .map(|m| m.text.trim().to_string())
        .filter(|m| !m.is_empty());
    Some(message.unwrap_or_else(|| format!("error {}", error.attr("code").unwrap_or("?"))))
}

/// Frames of a `stack_get` response, innermost first.
pub(crate) fn parse_stack(response: &Element) -> Vec<Frame> {
    response
        .children_named("stack")
        .map(|s| Frame {
            level: s.attr("level").and_then(|l| l.parse().ok()).unwrap_or(0),
            function: s.attr("where").unwrap_or("{main}").to_string(),
            file: uri_path(s.attr("filename").unwrap_or_default()),
            line: s
                .attr("lineno")
                .and_then(|l| l.parse::<usize>().ok())
                .unwrap_or(1)
                .saturating_sub(1),
        })
        .collect()
}

/// A variable from a `property` element, with its inline members.
pub(crate) fn parse_property(property: &Element) -> Variable {
    let kind = property.attr("type").unwrap_or_default();
    let raw = if property.attr("encoding") == Some("base64") {
        BASE64
            .decode(property.text.trim())
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default()
    } else {
        property.text.clone()
    };
    let value = match kind {
        "string" => format!("\"{raw}\""),
        "null" | "uninitialized" => kind.to_string(),
        "array" => format!("[{}]", property.attr("numchildren").unwrap_or("0")),
        "object" => String::new(),
        _ => raw,
    };
    let name = property.attr("name").unwrap_or_default().to_string();
    Variable {
        fullname: property
            .attr("fullname")
            .map_or_else(|| name.clone(), str::to_string),
        name,
        kind: property
            .attr("classname")
            .filter(|_| kind == "object")
            .unwrap_or(kind)
            .to_string(),
        value,
        has_children: property.attr("children") == Some("1"),
        children: property
            .children_named("property")
            .map(parse_property)
            .collect(),
    }
}

/// Variables of a `context_get` response.
pub(crate) fn parse_context(response: &Element) -> Vec<Variable> {
    response
        .children_named("property")
        .map(parse_property)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packet_reader_should_split_packets_across_reads() {
        let mut r = PacketReader::default();
        r.push(b"4\0<a/>");
        assert_eq!(r.next_packet(), None);
        r.push(b"\x003\0<b>");
        assert_eq!(r.next_packet().as_deref(), Some("<a/>"));
        r.push(b"\0");
        assert_eq!(r.next_packet().as_deref(), Some("<b>"));
    }

    #[test]
    fn encode_command_should_append_base64_data() {
        assert_eq!(encode_command("run", 3, "", None), b"run -i 3\0");
        assert_eq!(
            encode_command("eval", 4, "", Some("1+1")),
            b"eval -i 4 -- MSsx\0"
        );
    }

    #[test]
    fn uris_should_round_trip_paths_with_spaces() {
        let path = Path::new("/Users/me/My Project/tests/UserTest.php");
        assert_eq!(
            file_uri(path),
            "file:///Users/me/My%20Project/tests/UserTest.php"
        );
        assert_eq!(uri_path(&file_uri(path)), path);
    }

    #[test]
    fn responses_should_parse_into_frames_and_variables() {
        let stack = parse_xml(r#"<response xmlns="urn:debugger_protocol_v1" command="stack_get" transaction_id="5"><stack where="{closure:/app/tests/UserTest.php:5-8}" level="0" type="file" filename="file:///app/tests/UserTest.php" lineno="6"></stack><stack where="{main}" level="1" type="file" filename="file:///app/vendor/bin/pest" lineno="1"></stack></response>"#).unwrap();
        let frames = parse_stack(&stack);
        assert_eq!(frames.len(), 2);
        assert_eq!(
            (frames[0].file.as_path(), frames[0].line),
            (Path::new("/app/tests/UserTest.php"), 5)
        );

        let context = parse_xml(r#"<response command="context_get" transaction_id="6"><property name="$name" fullname="$name" type="string" size="3" encoding="base64"><![CDATA[QW5h]]></property><property name="$user" fullname="$user" type="object" classname="App\Models\User" children="1" numchildren="2"><property name="id" fullname="$user-&gt;id" type="int"><![CDATA[7]]></property></property></response>"#).unwrap();
        let vars = parse_context(&context);
        assert_eq!(vars[0].value, "\"Ana\"");
        assert_eq!(vars[1].kind, "App\\Models\\User");
        assert!(vars[1].has_children);
        assert_eq!(vars[1].children[0].fullname, "$user->id");
        assert_eq!(vars[1].children[0].value, "7");

        let error = parse_xml(r#"<response command="eval"><error code="206"><message><![CDATA[error evaluating code]]></message></error></response>"#).unwrap();
        assert_eq!(
            response_error(&error).as_deref(),
            Some("error evaluating code")
        );
    }
}
