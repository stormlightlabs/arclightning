//! JSON editing for `arcl setup` that keeps a settings file's own text.
//!
//! [`parse`] records where each value sits in the source, and [`insert_last`]
//! adds one value to an object or array by splicing new text in. Every byte
//! outside the insertion is copied as it was, so numbers, key order, escapes
//! and spacing in keys `setup` never touches survive a rewrite exactly.
//! [`Json`] builds the new value.

use serde::ser::{Serialize, SerializeMap, SerializeSeq, Serializer};

/// A value `setup` adds to a settings file. Objects keep their entries in
/// the order given.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Json {
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    /// The value as compact JSON, or as pretty JSON indented by `unit`.
    pub(crate) fn render(&self, unit: Option<&str>) -> String {
        let mut out = Vec::new();
        let written = match unit {
            Some(unit) => {
                let formatter = serde_json::ser::PrettyFormatter::with_indent(unit.as_bytes());
                self.serialize(&mut serde_json::Serializer::with_formatter(&mut out, formatter))
            }
            None => self.serialize(&mut serde_json::Serializer::new(&mut out)),
        };
        // Strings, arrays and objects of strings always serialize.
        written.expect("a setup value serializes");
        String::from_utf8(out).expect("serde_json writes UTF-8")
    }
}

impl Serialize for Json {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::String(value) => serializer.serialize_str(value),
            Self::Array(values) => {
                let mut seq = serializer.serialize_seq(Some(values.len()))?;
                for value in values {
                    seq.serialize_element(value)?;
                }
                seq.end()
            }
            Self::Object(entries) => {
                let mut map = serializer.serialize_map(Some(entries.len()))?;
                for (key, value) in entries {
                    map.serialize_entry(key, value)?;
                }
                map.end()
            }
        }
    }
}

/// One value in a parsed file, with the byte range of its source text.
#[derive(Debug)]
pub(crate) struct Node {
    /// The offset of the value's first byte.
    pub(crate) start: usize,
    /// The offset just past the value's last byte.
    pub(crate) end: usize,
    pub(crate) value: Value,
}

impl Node {
    /// The value under `key`, when this is an object that has one. With a
    /// repeated key this is the first, so callers check
    /// [`Node::duplicate_key`] before relying on it.
    pub(crate) fn get(&self, key: &str) -> Option<&Node> {
        match &self.value {
            Value::Object(members) => members
                .iter()
                .find(|member| member.key == key)
                .map(|member| &member.value),
            _ => None,
        }
    }

    /// The first key that appears more than once, when this is an object.
    pub(crate) fn duplicate_key(&self) -> Option<&str> {
        let Value::Object(members) = &self.value else {
            return None;
        };
        members
            .iter()
            .enumerate()
            .find(|(index, member)| members[..*index].iter().any(|earlier| earlier.key == member.key))
            .map(|(_, member)| member.key.as_str())
    }

    /// The string, when this is one.
    pub(crate) fn as_str(&self) -> Option<&str> {
        match &self.value {
            Value::String(value) => Some(value),
            _ => None,
        }
    }

    /// The elements, when this is an array.
    pub(crate) fn as_array(&self) -> Option<&[Node]> {
        match &self.value {
            Value::Array(values) => Some(values),
            _ => None,
        }
    }

    /// A short name for the value's type, for error messages.
    pub(crate) const fn kind(&self) -> &'static str {
        match self.value {
            Value::Scalar => "a number, boolean or null",
            Value::String(_) => "a string",
            Value::Array(_) => "an array",
            Value::Object(_) => "an object",
        }
    }
}

/// What a [`Node`] holds. Numbers, booleans and null stay as source text.
#[derive(Debug)]
pub(crate) enum Value {
    Scalar,
    String(String),
    Array(Vec<Node>),
    Object(Vec<Member>),
}

/// One `"key": value` pair in an object.
#[derive(Debug)]
pub(crate) struct Member {
    /// The offset of the key's opening quote.
    pub(crate) key_start: usize,
    pub(crate) key: String,
    pub(crate) value: Node,
}

/// Parse `text` as one JSON value, keeping where each value sits.
pub(crate) fn parse(text: &str) -> Result<Node, String> {
    // serde_json reports syntax errors with a line and column, and bounds the
    // nesting depth the recursive parser below then walks.
    serde_json::from_str::<serde::de::IgnoredAny>(text).map_err(|error| error.to_string())?;
    let mut parser = Parser { text, pos: 0 };
    let node = parser.value()?;
    parser.skip_whitespace();
    if parser.pos == text.len() { Ok(node) } else { Err(parser.unexpected()) }
}

/// `text` with `value` added as the last entry of `container`, an object
/// (`key` is `Some`) or an array (`key` is `None`). The new text follows the
/// container's layout: compact when the file is one line or the container
/// has entries on one line, otherwise one entry per line, indented like its
/// siblings or by `unit` past the container's own line.
pub(crate) fn insert_last(text: &str, container: &Node, key: Option<&str>, value: &Json, unit: &str) -> String {
    let eol = line_ending(text);
    let entry = |rendered: String| match key {
        Some(key) => format!("{}: {rendered}", Json::String(key.to_owned()).render(None)),
        None => rendered,
    };
    let last = match &container.value {
        Value::Object(members) => members.last().map(|member| (member.key_start, member.value.end)),
        Value::Array(values) => values.last().map(|value| (value.start, value.end)),
        _ => None,
    };
    let source = &text[container.start..container.end];
    let compact = !text.trim().contains('\n') || (last.is_some() && !source.contains('\n'));
    if compact {
        let entry = entry(value.render(None));
        return match last {
            Some((_, end)) => format!("{}, {entry}{}", &text[..end], &text[end..]),
            None => format!("{}{entry}{}", &text[..=container.start], &text[container.end - 1..]),
        };
    }
    let pretty = |indent: &str| {
        entry(value.render(Some(unit)))
            .split('\n')
            .collect::<Vec<_>>()
            .join(&format!("{eol}{indent}"))
    };
    match last {
        Some((start, end)) => {
            let indent = line_indent(text, start);
            format!("{},{eol}{indent}{}{}", &text[..end], pretty(indent), &text[end..])
        }
        None => {
            let base = line_indent(text, container.start);
            let indent = format!("{base}{unit}");
            format!(
                "{}{eol}{indent}{}{eol}{base}{}",
                &text[..=container.start],
                pretty(&indent),
                &text[container.end - 1..]
            )
        }
    }
}

/// `"\r\n"` when the first line of `text` ends with it, otherwise `"\n"`.
pub(crate) fn line_ending(text: &str) -> &'static str {
    match text.find('\n') {
        Some(index) if text[..index].ends_with('\r') => "\r\n",
        _ => "\n",
    }
}

/// The spaces and tabs that open the line holding byte `offset`.
fn line_indent(text: &str, offset: usize) -> &str {
    let line_start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
    let line = &text[line_start..offset];
    &line[..line.len() - line.trim_start_matches([' ', '\t']).len()]
}

/// A recursive-descent reader over text serde_json has already accepted.
struct Parser<'a> {
    text: &'a str,
    pos: usize,
}

impl Parser<'_> {
    /// The byte at the cursor, if any.
    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.pos).copied()
    }

    /// Move past spaces, tabs and line breaks.
    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    /// Move past `byte` after any whitespace, or fail.
    fn expect(&mut self, byte: u8) -> Result<(), String> {
        self.skip_whitespace();
        if self.peek() == Some(byte) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.unexpected())
        }
    }

    /// An error naming the cursor's offset.
    fn unexpected(&self) -> String {
        format!("unexpected input at byte {}", self.pos)
    }

    /// Read one value after any whitespace.
    fn value(&mut self) -> Result<Node, String> {
        self.skip_whitespace();
        let start = self.pos;
        let value = match self.peek() {
            Some(b'{') => Value::Object(self.object()?),
            Some(b'[') => Value::Array(self.array()?),
            Some(b'"') => Value::String(self.string()?),
            Some(_) => {
                while !matches!(
                    self.peek(),
                    None | Some(b',' | b']' | b'}' | b' ' | b'\t' | b'\n' | b'\r')
                ) {
                    self.pos += 1;
                }
                if self.pos == start {
                    return Err(self.unexpected());
                }
                Value::Scalar
            }
            None => return Err(self.unexpected()),
        };
        Ok(Node { start, end: self.pos, value })
    }

    /// Read a string token from its opening quote and decode it.
    fn string(&mut self) -> Result<String, String> {
        let start = self.pos;
        self.pos += 1;
        loop {
            match self.peek() {
                Some(b'\\') => self.pos += 2,
                Some(b'"') => break,
                Some(_) => self.pos += 1,
                None => return Err(self.unexpected()),
            }
        }
        self.pos += 1;
        serde_json::from_str(&self.text[start..self.pos]).map_err(|error| error.to_string())
    }

    /// Read an array from its `[`.
    fn array(&mut self) -> Result<Vec<Node>, String> {
        self.pos += 1;
        let mut values = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(values);
        }
        loop {
            values.push(self.value()?);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(values);
                }
                _ => return Err(self.unexpected()),
            }
        }
    }

    /// Read an object from its `{`.
    fn object(&mut self) -> Result<Vec<Member>, String> {
        self.pos += 1;
        let mut members = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(members);
        }
        loop {
            self.skip_whitespace();
            if self.peek() != Some(b'"') {
                return Err(self.unexpected());
            }
            let key_start = self.pos;
            let key = self.string()?;
            self.expect(b':')?;
            let value = self.value()?;
            members.push(Member { key_start, key, value });
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(members);
                }
                _ => return Err(self.unexpected()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> Json {
        Json::Object(vec![("a".to_owned(), Json::Array(vec![Json::String("b".to_owned())]))])
    }

    #[test]
    fn values_keep_their_source_ranges() {
        let text = r#" {"zeta": 1E5, "alpha": {"b": [true, null, "x\"y"]}} "#;
        let root = parse(text).expect("parses");
        let zeta = root.get("zeta").expect("zeta");
        assert_eq!(&text[zeta.start..zeta.end], "1E5");
        let b = root.get("alpha").and_then(|alpha| alpha.get("b")).expect("b");
        let values = b.as_array().expect("array");
        assert_eq!(values.len(), 3);
        assert_eq!(values[2].as_str(), Some("x\"y"));
    }

    #[test]
    fn invalid_json_is_an_error() {
        for text in ["{", "{\"a\": 1,}", "[1] 2", ""] {
            assert!(parse(text).is_err(), "{text}");
        }
    }

    #[test]
    fn a_repeated_key_is_found() {
        let root = parse(r#"{"a": 1, "b": 2, "a": 3}"#).expect("parses");
        assert_eq!(root.duplicate_key(), Some("a"));
        assert_eq!(parse(r#"{"a": 1}"#).expect("parses").duplicate_key(), None);
    }

    #[test]
    fn an_insert_copies_every_other_byte() {
        let text = "{\n  \"big\": 12345678901234567890123,\n  \"e\": 1E5\n}\n";
        let root = parse(text).expect("parses");
        let after = insert_last(text, &root, Some("new"), &entry(), "  ");
        assert_eq!(
            after,
            "{\n  \"big\": 12345678901234567890123,\n  \"e\": 1E5,\n  \"new\": {\n    \"a\": [\n      \"b\"\n    ]\n  }\n}\n"
        );
    }

    #[test]
    fn an_insert_into_an_empty_container_indents_past_its_line() {
        let text = "{\n\t\"list\": []\n}";
        let root = parse(text).expect("parses");
        let list = root.get("list").expect("list");
        let after = insert_last(text, list, None, &Json::String("x".to_owned()), "\t");
        assert_eq!(after, "{\n\t\"list\": [\n\t\t\"x\"\n\t]\n}");
    }

    #[test]
    fn an_insert_on_one_line_stays_on_one_line() {
        let text = "{\n  \"hooks\": {\"Stop\": []}\n}\n";
        let root = parse(text).expect("parses");
        let hooks = root.get("hooks").expect("hooks");
        let after = insert_last(text, hooks, Some("new"), &entry(), "  ");
        assert_eq!(after, "{\n  \"hooks\": {\"Stop\": [], \"new\": {\"a\":[\"b\"]}}\n}\n");

        let text = "{}";
        let after = insert_last(text, &parse(text).expect("parses"), Some("new"), &entry(), "\t");
        assert_eq!(after, "{\"new\": {\"a\":[\"b\"]}}");
    }

    #[test]
    fn an_insert_keeps_crlf_line_endings() {
        let text = "{\r\n  \"a\": 1\r\n}\r\n";
        let root = parse(text).expect("parses");
        let after = insert_last(text, &root, Some("b"), &entry(), "  ");
        assert_eq!(after.matches('\n').count(), after.matches("\r\n").count(), "{after:?}");
        assert!(after.starts_with("{\r\n  \"a\": 1,\r\n  \"b\": {\r\n"), "{after:?}");
    }
}
