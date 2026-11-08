//! The `SessionStart` hook `arcl setup` adds to a JSON settings file.
//!
//! `jsonc-parser`'s concrete syntax tree holds every byte of the file, so an
//! insert changes only the containers on the path to the hook. Numbers, key
//! order, escapes and spacing elsewhere survive a rewrite exactly. New text
//! takes the file's indentation and line ending.
//!
//! `.claude/settings.json` and `.codex/hooks.json` are strict JSON, so
//! comments, trailing commas and other JSONC syntax are refused rather than
//! kept: Claude Code and Codex would fail to read the result.

use jsonc_parser::ParseOptions;
use jsonc_parser::cst::{CstArray, CstInputValue, CstNode, CstObject, CstRootNode};

use super::*;

/// The settings file with a `SessionStart` hook that runs [`HOOK_COMMAND`],
/// unless [`has_hook`] finds one. An empty or missing file becomes a new
/// object with two-space indentation. A repeated key on the path to the hook
/// is an error: JSON readers keep the last one, and the hook could land in a
/// copy the agent ignores.
pub(super) fn with_hook(before: Option<&str>, matcher: Option<&str>, path: &Path) -> CResult<String> {
    let invalid = |message: String| CommandError::InvalidSetupJson { path: path.to_owned(), message };
    let text = before.unwrap_or("");
    let mut entry = Vec::new();
    if let Some(matcher) = matcher {
        entry.push(("matcher".to_owned(), CstInputValue::String(matcher.to_owned())));
    }
    let command = CstInputValue::Object(vec![
        ("type".to_owned(), CstInputValue::String("command".to_owned())),
        ("command".to_owned(), CstInputValue::String(HOOK_COMMAND.to_owned())),
    ]);
    entry.push(("hooks".to_owned(), CstInputValue::Array(vec![command])));
    let entry = CstInputValue::Object(entry);
    let session_start = || {
        CstInputValue::Object(vec![(
            "SessionStart".to_owned(),
            CstInputValue::Array(vec![entry.clone()]),
        )])
    };

    if text.trim().is_empty() {
        let root = parse("").map_err(invalid)?;
        root.set_value(CstInputValue::Object(vec![("hooks".to_owned(), session_start())]));
        return Ok(root.to_string());
    }
    let root = parse(text).map_err(invalid)?;
    let Some(settings) = root.object_value() else {
        let kind = root.value().map_or("nothing", |value| kind(&value));
        return Err(invalid(format!("the file holds {kind}, not an object")));
    };
    unique_keys(&settings, "", path)?;
    let Some(hooks) = settings.get("hooks").and_then(|prop| prop.value()) else {
        settings.ensure_multiline();
        settings.append("hooks", session_start());
        return Ok(root.to_string());
    };
    let Some(hooks) = hooks.as_object() else {
        return Err(invalid(format!("`hooks` is {}, not an object", kind(&hooks))));
    };
    unique_keys(&hooks, "hooks.", path)?;
    let Some(events) = hooks.get("SessionStart").and_then(|prop| prop.value()) else {
        settings.ensure_multiline();
        hooks.ensure_multiline();
        hooks.append("SessionStart", CstInputValue::Array(vec![entry]));
        return Ok(root.to_string());
    };
    let Some(events) = events.as_array() else {
        return Err(invalid(format!(
            "`hooks.SessionStart` is {}, not an array",
            kind(&events)
        )));
    };
    let entries = events.elements();
    for (index, existing) in entries
        .iter()
        .enumerate()
        .filter_map(|(index, node)| node.as_object().map(|object| (index, object)))
    {
        let prefix = format!("hooks.SessionStart[{index}].");
        unique_keys(&existing, &prefix, path)?;
        for (inner, hook) in array(&existing, "hooks")
            .iter()
            .enumerate()
            .filter_map(|(inner, node)| node.as_object().map(|object| (inner, object)))
        {
            unique_keys(&hook, &format!("{prefix}hooks[{inner}]."), path)?;
        }
    }
    if has_hook(&entries) {
        return Ok(text.to_owned());
    }
    settings.ensure_multiline();
    hooks.ensure_multiline();
    events.ensure_multiline();
    events.append(entry);
    Ok(root.to_string())
}

/// `text` as a syntax tree, after serde_json accepts it as strict JSON.
/// serde_json refuses what `jsonc-parser` would let through, such as
/// comments, trailing commas, control characters in strings and whitespace
/// outside JSON's four, and it reports the line and column. Parsing into a
/// [`serde_json::Value`] also stops at 128 levels of nesting, before
/// `jsonc-parser`'s recursive parser walks the text.
fn parse(text: &str) -> Result<CstRootNode, String> {
    if !text.is_empty() {
        serde_json::from_str::<serde_json::Value>(text).map_err(|error| error.to_string())?;
    }
    let strict = ParseOptions {
        allow_comments: false,
        allow_loose_object_property_names: false,
        allow_trailing_commas: false,
        allow_missing_commas: false,
        allow_single_quoted_strings: false,
        allow_hexadecimal_numbers: false,
        allow_unary_plus_numbers: false,
        allow_bare_decimal_point_numbers: false,
        allow_non_finite_numbers: false,
        allow_extended_string_escapes: false,
    };
    CstRootNode::parse(text, &strict).map_err(|error| error.to_string())
}

/// Fail when `object` has a key more than once. `prefix` is the path to
/// `object`, for the message.
fn unique_keys(object: &CstObject, prefix: &str, path: &Path) -> CResult<()> {
    let mut seen = std::collections::HashSet::new();
    let repeated = object
        .properties()
        .iter()
        .filter_map(|prop| prop.decoded_name())
        .find(|name| !seen.insert(name.clone()));
    match repeated {
        Some(key) => Err(CommandError::DuplicateSetupJsonKey { path: path.to_owned(), key: format!("{prefix}{key}") }),
        None => Ok(()),
    }
}

/// Whether a `SessionStart` entry already runs [`HOOK_COMMAND`] when a
/// session starts: its command is exactly that, and its matcher, if any,
/// covers `startup`.
fn has_hook(entries: &[CstNode]) -> bool {
    entries
        .iter()
        .filter_map(CstNode::as_object)
        .filter(|entry| match entry.get("matcher").and_then(|prop| prop.value()) {
            None => true,
            Some(matcher) => string(&matcher).is_some_and(|matcher| covers_startup(&matcher)),
        })
        .flat_map(|entry| array(&entry, "hooks"))
        .filter_map(|hook| hook.as_object()?.get("command")?.value())
        .filter_map(|command| string(&command))
        .any(|command| command.trim() == HOOK_COMMAND)
}

/// Whether a `SessionStart` matcher selects sessions that start fresh: an
/// empty matcher, `*`, or a `|` list that names `startup`.
fn covers_startup(matcher: &str) -> bool {
    let matcher = matcher.trim();
    matcher.is_empty() || matcher == "*" || matcher.split('|').any(|part| part.trim() == "startup")
}

/// The elements of the array under `key` in `object`, or none when it is
/// missing or not an array.
fn array(object: &CstObject, key: &str) -> Vec<CstNode> {
    object
        .get(key)
        .and_then(|prop| prop.value())
        .and_then(|value| value.as_array())
        .as_ref()
        .map_or_else(Vec::new, CstArray::elements)
}

/// The decoded string, when `node` is one.
fn string(node: &CstNode) -> Option<String> {
    node.as_string_lit()?.decoded_value().ok()
}

/// A short name for the type of `node`, for error messages.
fn kind(node: &CstNode) -> &'static str {
    if node.as_object().is_some() {
        "an object"
    } else if node.as_array().is_some() {
        "an array"
    } else if node.as_string_lit().is_some() {
        "a string"
    } else {
        "a number, boolean or null"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path() -> PathBuf {
        PathBuf::from("settings.json")
    }

    /// The hook entry as `with_hook` writes it for a pretty file indented by
    /// `unit`, opening at `depth` levels.
    fn added(unit: &str, depth: usize, eol: &str) -> String {
        let lines = [
            "{",
            "\t\"hooks\": [",
            "\t\t{",
            "\t\t\t\"type\": \"command\",",
            "\t\t\t\"command\": \"if command -v arcl >/dev/null; then arcl prime; fi\"",
            "\t\t}",
            "\t]",
            "}",
        ];
        let indent = unit.repeat(depth);
        lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                let line = line.replace('\t', unit);
                if index == 0 { line } else { format!("{indent}{line}") }
            })
            .collect::<Vec<_>>()
            .join(eol)
    }

    #[test]
    fn the_hook_merges_into_existing_settings_in_key_order() {
        let before = "{\n  \"permissions\": {\"allow\": [\"Bash(ls)\"]},\n  \"hooks\": {\"Stop\": []}\n}\n";
        let after = with_hook(Some(before), None, &path()).expect("merges");
        assert!(
            after.starts_with("{\n  \"permissions\": {\"allow\": [\"Bash(ls)\"]},\n  \"hooks\": {\n    \"Stop\": [],\n    \"SessionStart\": ["),
            "{after}"
        );
        assert_eq!(with_hook(Some(&after), None, &path()).expect("second run"), after);
    }

    #[test]
    fn a_new_file_is_an_object_with_the_hook() {
        for before in [None, Some(""), Some(" \n")] {
            let after = with_hook(before, None, &path()).expect("creates");
            let expected = format!(
                "{{\n  \"hooks\": {{\n    \"SessionStart\": [\n      {}\n    ]\n  }}\n}}\n",
                added("  ", 3, "\n")
            );
            assert_eq!(after, expected);
        }
    }

    #[test]
    fn a_settings_file_that_is_not_an_object_is_an_error() {
        for text in ["[]", "1", "{\"hooks\": []}", "{\"hooks\": {\"SessionStart\": {}}}", "{"] {
            let error = with_hook(Some(text), None, &path()).expect_err(text);
            assert!(matches!(error, CommandError::InvalidSetupJson { .. }), "{text}");
        }
    }

    #[test]
    fn comments_trailing_commas_and_other_jsonc_are_refused() {
        for text in [
            "{\n  // a comment\n  \"model\": \"x\"\n}\n",
            "{\"model\": \"x\" /* a comment */}",
            "{\"model\": \"x\",}",
            "{\"list\": [1,]}",
            "{model: \"x\"}",
            "{'model': 'x'}",
            "{\"model\": \"x\" \"other\": 1}",
            "{\"n\": 0x1F}",
            "{\"n\": .5}",
            "{\"n\": NaN}",
            "{\"s\": \"tab\there\"}",
            "{\u{a0}\"model\": \"x\"}",
            "\u{feff}{}",
        ] {
            let error = with_hook(Some(text), None, &path()).expect_err(text);
            assert!(matches!(error, CommandError::InvalidSetupJson { .. }), "{text:?}");
        }
    }

    #[test]
    fn an_invalid_file_names_the_line_and_column() {
        let error = with_hook(Some("{\n  \"a\": 1,\n}\n"), None, &path()).expect_err("refuses");
        assert!(error.to_string().contains("line 3 column 1"), "{error}");
    }

    #[test]
    fn a_repeated_key_on_the_hook_path_is_an_error() {
        for (text, key) in [
            (r#"{"hooks": {}, "hooks": {"Stop": []}}"#, "`hooks`"),
            (
                r#"{"hooks": {"SessionStart": [], "SessionStart": []}}"#,
                "`hooks.SessionStart`",
            ),
            (
                r#"{"hooks": {"SessionStart": [{"hooks": [], "hooks": []}]}}"#,
                "`hooks.SessionStart[0].hooks`",
            ),
            (
                r#"{"hooks": {"SessionStart": [{"hooks": [{"command": "a", "command": "b"}]}]}}"#,
                "`hooks.SessionStart[0].hooks[0].command`",
            ),
            (r#"{"hooks": {}, "\u0068ooks": {}}"#, "`hooks`"),
            (
                r#"{"hooks": {"SessionStart": [1, {"hooks": [], "hooks": []}]}}"#,
                "`hooks.SessionStart[1].hooks`",
            ),
        ] {
            let error = with_hook(Some(text), None, &path()).expect_err(text);
            assert!(matches!(error, CommandError::DuplicateSetupJsonKey { .. }), "{text}");
            let message = error.to_string();
            assert!(message.contains("settings.json") && message.contains(key), "{message}");
        }
    }

    #[test]
    fn numbers_setup_does_not_touch_keep_their_text() {
        let before = "{\n  \"big\": 12345678901234567890123,\n  \"small\": 1E5\n}\n";
        let after = with_hook(Some(before), None, &path()).expect("merges");
        assert!(
            after.starts_with("{\n  \"big\": 12345678901234567890123,\n  \"small\": 1E5,\n  \"hooks\": {"),
            "{after}"
        );
        assert_eq!(with_hook(Some(&after), None, &path()).expect("second run"), after);
    }

    #[test]
    fn every_byte_outside_the_insert_is_kept() {
        let before = "{\n  \"e\": \"\\u00e9\\/\",  \"n\": -0.0e-0 ,\n\n  \"hooks\" :  {\n    \"Stop\": [ ]\n  }\n}";
        let after = with_hook(Some(before), None, &path()).expect("merges");
        let expected = format!(
            "{{\n  \"e\": \"\\u00e9\\/\",  \"n\": -0.0e-0 ,\n\n  \"hooks\" :  {{\n    \"Stop\": [ ],\n    \"SessionStart\": [\n      {}\n    ]\n  }}\n}}",
            added("  ", 3, "\n")
        );
        assert_eq!(after, expected);
    }

    #[test]
    fn the_hook_takes_the_file_indentation() {
        for unit in ["\t", "    "] {
            let before = format!("{{\n{unit}\"hooks\": {{\n{unit}{unit}\"SessionStart\": []\n{unit}}}\n}}\n");
            let after = with_hook(Some(&before), Some("startup"), &path()).expect("merges");
            let entry =
                added(unit, 3, "\n").replacen("{\n", &format!("{{\n{}\"matcher\": \"startup\",\n", unit.repeat(4)), 1);
            let expected = format!(
                "{{\n{unit}\"hooks\": {{\n{unit}{unit}\"SessionStart\": [\n{}{entry}\n{unit}{unit}]\n{unit}}}\n}}\n",
                unit.repeat(3)
            );
            assert_eq!(after, expected, "{unit:?}");
        }
    }

    #[test]
    fn the_hook_keeps_crlf_line_endings() {
        let before = "{\r\n  \"a\": 1\r\n}\r\n";
        let after = with_hook(Some(before), None, &path()).expect("merges");
        assert_eq!(after.matches('\n').count(), after.matches("\r\n").count(), "{after:?}");
        assert!(after.starts_with("{\r\n  \"a\": 1,\r\n  \"hooks\": {\r\n"), "{after:?}");
        assert_eq!(with_hook(Some(&after), None, &path()).expect("second run"), after);
    }

    #[test]
    fn a_one_line_file_spreads_the_containers_on_the_hook_path() {
        let before = r#"{"model": "x", "hooks": {"SessionStart": [{"hooks": []}]}}"#;
        let after = with_hook(Some(before), Some("startup"), &path()).expect("merges");
        let entry = added("  ", 3, "\n").replacen("{\n", "{\n        \"matcher\": \"startup\",\n", 1);
        assert_eq!(
            after,
            format!(
                "{{\n  \"model\": \"x\",\n  \"hooks\": {{\n    \"SessionStart\": [\n      {{\"hooks\": []}},\n      {entry}\n    ]\n  }}\n}}"
            )
        );
    }

    #[test]
    fn only_the_exact_command_with_a_startup_matcher_counts_as_the_hook() {
        let settings = |matcher: &str, command: &str| {
            format!(
                r#"{{"hooks": {{"SessionStart": [{{{matcher}"hooks": [{{"type": "command", "command": {command:?}}}]}}]}}}}"#
            )
        };
        let present = [
            settings("", HOOK_COMMAND),
            settings(r#""matcher": "startup|resume", "#, &format!(" {HOOK_COMMAND} ")),
            settings(r#""matcher": "*", "#, HOOK_COMMAND),
        ];
        for text in present {
            assert_eq!(
                with_hook(Some(&text), Some("startup|resume"), &path()).expect("reads"),
                text
            );
        }
        let absent = [
            settings("", "echo \"run arcl prime\""),
            settings(r#""matcher": "compact", "#, HOOK_COMMAND),
            settings(r#""matcher": 1, "#, HOOK_COMMAND),
        ];
        for text in absent {
            let after = with_hook(Some(&text), Some("startup|resume"), &path()).expect("adds");
            assert_ne!(after, text);
            assert_eq!(
                after.matches(HOOK_COMMAND).count(),
                1 + text.matches(HOOK_COMMAND).count(),
                "{after}"
            );
        }
    }
}
