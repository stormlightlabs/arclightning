//! Short record IDs: the end of a record's ULID, long enough to name one
//! record.
//!
//! A ULID's first 10 characters are a timestamp, so records made together
//! share a long prefix. The last 16 are random, so a suffix tells records
//! apart. A short ID is such a suffix, optionally after the record's kind, as
//! in `535B` or `t-535B`.
//!
//! Since records share their first characters, input that matches the start
//! of a record's ULID is refused rather than read as a suffix, so a prefix
//! typed out of habit never acts on a record that happens to end in it.

use std::collections::HashMap;

use thiserror::Error;

/// The fewest ULID characters a short ID can have.
pub const MIN_SHORT_ID_LEN: usize = 4;

/// The prefix every full record ID starts with.
const FULL_ID_PREFIX: &str = "arcl-";

/// The length of a canonical ULID.
const ULID_LEN: usize = 26;

/// The kinds a short ID can name, as they appear in full IDs: `t` in
/// `arcl-t-…`.
const KINDS: [&str; 8] = ["c", "r", "s", "pl", "ph", "t", "n", "g"];

/// The Crockford base32 alphabet ULIDs use.
const CROCKFORD: &str = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// How many records a [`ShortIdError::Prefix`] message lists.
const PREFIX_EXAMPLES: usize = 3;

/// Why a short ID names no record, or more than one.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ShortIdError {
    /// Fewer than [`MIN_SHORT_ID_LEN`] ULID characters.
    #[error("short ID `{input}` is too short; give at least {MIN_SHORT_ID_LEN} characters from the end of the ID")]
    TooShort { input: String },
    /// A character outside Crockford base32, or more characters than a ULID.
    #[error("`{input}` is not a record ID; give a full ID or the end of one, such as `535B` or `t-535B`")]
    Invalid { input: String },
    /// A kind before the dash that no record ID uses.
    #[error("short ID `{input}` names unknown kind `{kind}`; use c, r, s, pl, ph, t, n or g")]
    UnknownKind { input: String, kind: String },
    /// The input matches the start of a record's ULID, not only its end.
    /// `matches` lists the records whose ULIDs start with it.
    #[error("`{input}` is the start of a record ID, but a short ID is the end of one; give the last characters instead:{}", list_suffixes(.matches))]
    Prefix { input: String, matches: Vec<RecordRef> },
    /// No record of the requested kind ends in the short ID.
    #[error("no record matches short ID `{input}`")]
    NotFound { input: String },
    /// More than one record ends in the short ID; `matches` lists them.
    #[error("short ID `{input}` matches more than one record; give more characters:{}", list_matches(.matches))]
    Ambiguous { input: String, matches: Vec<RecordRef> },
}

impl ShortIdError {
    /// Whether no record matched, as opposed to input that cannot match.
    pub const fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound { .. })
    }
}

/// A record's full ID and title, the input short-ID resolution searches.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordRef {
    /// The full record ID, such as `arcl-t-01M5VDDD42HZQ1FEN49X80GBA5`.
    pub id: String,
    /// The record's title, shown when a short ID is ambiguous.
    pub title: String,
}

/// The shortest unique suffix of each record's ULID, at least
/// [`MIN_SHORT_ID_LEN`] characters, computed across every record as git
/// abbreviates commit hashes.
///
/// Records of different kinds can share a whole ULID, as when a workspace
/// import loads a copied file. Each of those shows as `<kind>-<ULID>`, such
/// as `t-01M5VDDD42HZQ1FEN49X80GBA5`, so every short ID names one record.
///
/// The default value knows no records and shows every ID in full.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ShortIds {
    /// How many characters from the end of each full ID its short ID keeps.
    lengths: HashMap<String, usize>,
}

impl ShortIds {
    /// Compute short IDs for `ids`, the full IDs of every record.
    pub fn new<I, S>(ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut reversed = ids
            .into_iter()
            .map(|id| {
                let id = id.into();
                (ulid_part(&id).chars().rev().collect::<String>(), id)
            })
            .collect::<Vec<_>>();
        reversed.sort();
        let mut forward = reversed.iter().map(|(_, id)| ulid_part(id)).collect::<Vec<_>>();
        forward.sort_unstable();
        forward.dedup();
        let common = |left: &str, right: &str| left.bytes().zip(right.bytes()).take_while(|(a, b)| a == b).count();
        let mut lengths = HashMap::with_capacity(reversed.len());
        for (index, (key, id)) in reversed.iter().enumerate() {
            let previous = index.checked_sub(1).map(|previous| reversed[previous].0.as_str());
            let next = reversed.get(index + 1).map(|next| next.0.as_str());
            let length = if previous == Some(key.as_str()) || next == Some(key.as_str()) {
                id.strip_prefix(FULL_ID_PREFIX).map_or(id.len(), str::len)
            } else {
                let before = previous.map_or(0, |previous| common(key, previous));
                let after = next.map_or(0, |next| common(key, next));
                let mut length = (before.max(after) + 1).max(MIN_SHORT_ID_LEN).min(key.len());
                let ulid = ulid_part(id);
                while length < ulid.len() && starts_some_ulid(&forward, &ulid[ulid.len() - length..]) {
                    length += 1;
                }
                length
            };
            lengths.insert(id.clone(), length);
        }
        Self { lengths }
    }

    /// The short ID for `id`, or `id` itself when it is not a known record.
    pub fn short<'a>(&self, id: &'a str) -> &'a str {
        match self.lengths.get(id) {
            Some(&length) => &id[id.len().saturating_sub(length)..],
            None => id,
        }
    }

    /// `text` with each known full record ID in it replaced by its short ID,
    /// for messages such as readiness reasons that name records.
    pub fn shorten_text(&self, text: &str) -> String {
        if self.lengths.is_empty() {
            return text.to_owned();
        }
        let mut shortened = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(start) = rest.find(FULL_ID_PREFIX) {
            shortened.push_str(&rest[..start]);
            let candidate = &rest[start..];
            let known = KINDS.into_iter().find_map(|kind| {
                candidate
                    .get(..FULL_ID_PREFIX.len() + kind.len() + 1 + ULID_LEN)
                    .filter(|id| self.lengths.contains_key(*id))
            });
            let taken = known.unwrap_or(FULL_ID_PREFIX);
            shortened.push_str(self.short(taken));
            rest = &candidate[taken.len()..];
        }
        shortened.push_str(rest);
        shortened
    }
}

/// Whether `input` is written as a full record ID, `arcl-<kind>-<ULID>`,
/// rather than a short ID. A full ID is parsed as it is, never resolved.
/// The `arcl-` prefix matches in any case, so `ARCL-T-…` reaches the full-ID
/// parser and fails there with its prefix error.
pub fn is_full_id(input: &str) -> bool {
    input
        .get(..FULL_ID_PREFIX.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(FULL_ID_PREFIX))
}

/// Find the one record whose ID ends in the short ID `input`.
///
/// `input` is the end of a ULID, at least [`MIN_SHORT_ID_LEN`] characters,
/// optionally after a kind and a dash: `535B` or `t-535B`. Letters match in
/// either case, and `I`, `L` and `O` read as `1`, `1` and `0`, as Crockford
/// base32 decodes them.
///
/// `expected` is the full-ID prefix the caller wants, such as `arcl-t-`.
/// When `input` names no kind, only records with that prefix match, so
/// `535B` finds the task even when a spec ends in `535B` too.
///
/// Input that also starts any record's ULID, such as `01M5`, fails with
/// [`ShortIdError::Prefix`] unless it is that whole ULID.
pub fn resolve_short_id<'a>(
    input: &str, records: &'a [RecordRef], expected: Option<&str>,
) -> Result<&'a RecordRef, ShortIdError> {
    let (kind, suffix) = match input.split_once('-') {
        Some((kind, suffix)) => {
            let kind = kind.to_ascii_lowercase();
            let kind = KINDS
                .into_iter()
                .find(|known| *known == kind)
                .ok_or_else(|| ShortIdError::UnknownKind { input: input.to_owned(), kind })?;
            (Some(kind), suffix)
        }
        None => (None, input),
    };
    let suffix = normalize(suffix).ok_or_else(|| ShortIdError::Invalid { input: input.to_owned() })?;
    if suffix.len() < MIN_SHORT_ID_LEN {
        return Err(ShortIdError::TooShort { input: input.to_owned() });
    }
    let starting = records
        .iter()
        .filter(|record| {
            let ulid = ulid_part(&record.id);
            ulid.starts_with(&suffix) && ulid != suffix
        })
        .cloned()
        .collect::<Vec<_>>();
    if !starting.is_empty() {
        return Err(ShortIdError::Prefix { input: input.to_owned(), matches: starting });
    }
    let prefix = kind.map(|kind| format!("{FULL_ID_PREFIX}{kind}-"));
    let prefix = prefix.as_deref().or(expected);
    let matches = records
        .iter()
        .filter(|record| ulid_part(&record.id).ends_with(&suffix))
        .filter(|record| prefix.is_none_or(|prefix| record.id.starts_with(prefix)))
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Err(ShortIdError::NotFound { input: input.to_owned() }),
        [record] => Ok(record),
        _ => Err(ShortIdError::Ambiguous { input: input.to_owned(), matches: matches.into_iter().cloned().collect() }),
    }
}

/// The ULID at the end of a full ID: the text after its last dash.
fn ulid_part(id: &str) -> &str {
    id.rsplit('-').next().unwrap_or(id)
}

/// Whether `text` starts some ULID in `sorted` other than `text` itself.
fn starts_some_ulid(sorted: &[&str], text: &str) -> bool {
    let index = sorted.partition_point(|ulid| *ulid < text);
    sorted[index..]
        .iter()
        .take_while(|ulid| ulid.starts_with(text))
        .any(|ulid| *ulid != text)
}

/// `suffix` in canonical Crockford form, or `None` when it has a character
/// outside the alphabet or is longer than a ULID.
fn normalize(suffix: &str) -> Option<String> {
    if suffix.len() > ULID_LEN {
        return None;
    }
    suffix
        .chars()
        .map(|character| match character.to_ascii_uppercase() {
            'I' | 'L' => Some('1'),
            'O' => Some('0'),
            upper => CROCKFORD.contains(upper).then_some(upper),
        })
        .collect()
}

/// The matches of an ambiguous short ID, one `<full ID> <title>` per line.
fn list_matches(matches: &[RecordRef]) -> String {
    matches
        .iter()
        .map(|record| format!("\n  {}  {}", record.id, record.title))
        .collect()
}

/// The records whose ULIDs start with typed input, each as its last
/// [`MIN_SHORT_ID_LEN`] characters, full ID and title, up to
/// [`PREFIX_EXAMPLES`] of them.
fn list_suffixes(matches: &[RecordRef]) -> String {
    let mut listed = matches
        .iter()
        .take(PREFIX_EXAMPLES)
        .map(|record| {
            let suffix = &record.id[record.id.len().saturating_sub(MIN_SHORT_ID_LEN)..];
            format!("\n  {suffix}  {}  {}", record.id, record.title)
        })
        .collect::<String>();
    if matches.len() > PREFIX_EXAMPLES {
        listed.push_str(&format!("\n  and {} more", matches.len() - PREFIX_EXAMPLES));
    }
    listed
}

#[cfg(test)]
mod tests {
    use super::{RecordRef, ShortIdError, ShortIds, is_full_id, resolve_short_id};

    fn record(id: &str, title: &str) -> RecordRef {
        RecordRef { id: id.to_owned(), title: title.to_owned() }
    }

    fn records() -> Vec<RecordRef> {
        vec![
            record("arcl-t-01M5VDDD42HZQ1FEN49X80535B", "First task"),
            record("arcl-s-01M5VDDD42HZQ1FEN49X81535B", "A spec"),
            record("arcl-t-01M5VDDD42HZQ1FEN49X80GBA5", "Second task"),
        ]
    }

    #[test]
    fn short_ids_take_four_characters_when_those_are_unique() {
        let ids = ShortIds::new(["arcl-t-01M5VDDD42HZQ1FEN49X80GBA5", "arcl-t-01M5VDDD42HZQ1FEN49X80535B"]);
        assert_eq!(ids.short("arcl-t-01M5VDDD42HZQ1FEN49X80GBA5"), "GBA5");
        assert_eq!(ids.short("arcl-t-01M5VDDD42HZQ1FEN49X80535B"), "535B");
    }

    #[test]
    fn short_ids_grow_until_records_sharing_a_suffix_differ() {
        let ids = ShortIds::new(records().into_iter().map(|record| record.id));
        assert_eq!(ids.short("arcl-t-01M5VDDD42HZQ1FEN49X80535B"), "0535B");
        assert_eq!(ids.short("arcl-s-01M5VDDD42HZQ1FEN49X81535B"), "1535B");
        assert_eq!(ids.short("arcl-t-01M5VDDD42HZQ1FEN49X80GBA5"), "GBA5");
    }

    #[test]
    fn short_ids_leave_unknown_ids_whole() {
        let ids = ShortIds::default();
        assert_eq!(
            ids.short("arcl-t-01M5VDDD42HZQ1FEN49X80GBA5"),
            "arcl-t-01M5VDDD42HZQ1FEN49X80GBA5"
        );
    }

    #[test]
    fn a_unique_suffix_resolves_in_either_case() {
        let records = records();
        assert_eq!(
            resolve_short_id("GBA5", &records, None).map(|record| record.title.as_str()),
            Ok("Second task")
        );
        assert_eq!(
            resolve_short_id("gba5", &records, None).map(|record| record.title.as_str()),
            Ok("Second task")
        );
        assert_eq!(
            resolve_short_id("0535b", &records, None).map(|record| record.title.as_str()),
            Ok("First task")
        );
        assert_eq!(
            resolve_short_id("o535b", &records, None).map(|record| record.title.as_str()),
            Ok("First task")
        );
    }

    #[test]
    fn an_ambiguous_suffix_lists_every_match() {
        let records = records();
        let error = resolve_short_id("535B", &records, None).expect_err("two records end in 535B");
        let ShortIdError::Ambiguous { matches, .. } = &error else {
            panic!("expected ambiguity, got {error:?}")
        };
        assert_eq!(matches.len(), 2);
        let message = error.to_string();
        assert!(
            message.contains("arcl-t-01M5VDDD42HZQ1FEN49X80535B  First task"),
            "{message}"
        );
        assert!(
            message.contains("arcl-s-01M5VDDD42HZQ1FEN49X81535B  A spec"),
            "{message}"
        );
    }

    #[test]
    fn a_kind_narrows_the_match() {
        let records = records();
        assert_eq!(
            resolve_short_id("t-535B", &records, None).map(|record| record.title.as_str()),
            Ok("First task")
        );
        assert_eq!(
            resolve_short_id("S-535b", &records, None).map(|record| record.title.as_str()),
            Ok("A spec")
        );
        assert!(matches!(
            resolve_short_id("pl-535B", &records, None),
            Err(ShortIdError::NotFound { .. })
        ));
        assert!(matches!(
            resolve_short_id("x-535B", &records, None),
            Err(ShortIdError::UnknownKind { .. })
        ));
    }

    #[test]
    fn short_and_invalid_input_is_refused() {
        let records = records();
        assert!(matches!(
            resolve_short_id("BA5", &records, None),
            Err(ShortIdError::TooShort { .. })
        ));
        assert!(matches!(
            resolve_short_id("t-", &records, None),
            Err(ShortIdError::TooShort { .. })
        ));
        assert!(matches!(
            resolve_short_id("GBU5", &records, None),
            Err(ShortIdError::Invalid { .. })
        ));
        assert!(matches!(
            resolve_short_id("ZZZZ", &records, None),
            Err(ShortIdError::NotFound { .. })
        ));
    }

    #[test]
    fn the_expected_kind_narrows_a_bare_suffix() {
        let records = records();
        assert_eq!(
            resolve_short_id("535B", &records, Some("arcl-t-")).map(|record| record.title.as_str()),
            Ok("First task")
        );
        assert_eq!(
            resolve_short_id("535B", &records, Some("arcl-s-")).map(|record| record.title.as_str()),
            Ok("A spec")
        );
        assert!(matches!(
            resolve_short_id("535B", &records, Some("arcl-n-")),
            Err(ShortIdError::NotFound { .. })
        ));
        assert_eq!(
            resolve_short_id("s-535B", &records, Some("arcl-t-")).map(|record| record.title.as_str()),
            Ok("A spec"),
            "a kind in the input wins, so the caller's parser reports the mismatch"
        );
    }

    #[test]
    fn records_sharing_a_whole_ulid_show_their_kind() {
        let records = vec![
            record("arcl-t-01M5VDDD42HZQ1FEN49X80535B", "A task"),
            record(
                "arcl-s-01M5VDDD42HZQ1FEN49X80535B",
                "A spec copied from the task's file",
            ),
            record("arcl-n-01M5VDDD42HZQ1FEN49X81535B", "A note"),
            record("arcl-pl-01M5VDDD42HZQ1FEN49X80GBA5", "A plan"),
        ];
        let ids = ShortIds::new(records.iter().map(|record| record.id.clone()));
        assert_eq!(
            ids.short("arcl-t-01M5VDDD42HZQ1FEN49X80535B"),
            "t-01M5VDDD42HZQ1FEN49X80535B"
        );
        assert_eq!(
            ids.short("arcl-s-01M5VDDD42HZQ1FEN49X80535B"),
            "s-01M5VDDD42HZQ1FEN49X80535B"
        );
        assert_eq!(ids.short("arcl-n-01M5VDDD42HZQ1FEN49X81535B"), "1535B");
        for record in &records {
            let short = ids.short(&record.id);
            assert_eq!(
                resolve_short_id(short, &records, None).map(|found| found.id.as_str()),
                Ok(record.id.as_str()),
                "{short} names {}",
                record.id
            );
        }
    }

    #[test]
    fn a_whole_ulid_resolves_as_a_suffix() {
        let records = records();
        let found = resolve_short_id("01M5VDDD42HZQ1FEN49X80GBA5", &records, None).expect("whole ULID matches");
        assert_eq!(found.id, "arcl-t-01M5VDDD42HZQ1FEN49X80GBA5");
    }

    #[test]
    fn input_that_starts_an_id_is_refused_with_suffixes_to_use() {
        let records = records();
        for input in ["01M5", "01m5v", "t-01M5VDDD"] {
            let error = resolve_short_id(input, &records, Some("arcl-t-")).expect_err("a prefix is refused");
            let ShortIdError::Prefix { matches, .. } = &error else {
                panic!("expected a prefix refusal for {input}, got {error:?}")
            };
            assert_eq!(matches.len(), 3, "{input}");
            let message = error.to_string();
            assert!(message.contains("the end of one"), "{message}");
            assert!(
                message.contains("GBA5  arcl-t-01M5VDDD42HZQ1FEN49X80GBA5  Second task"),
                "{message}"
            );
        }
    }

    #[test]
    fn short_ids_grow_past_the_start_of_another_id() {
        let ids = ShortIds::new(["arcl-t-01M5VDDD42HZQ1FEN49X8001M5", "arcl-t-01M5VDDD42HZQ1FEN49X80GBA5"]);
        assert_eq!(ids.short("arcl-t-01M5VDDD42HZQ1FEN49X8001M5"), "001M5");
        let records = vec![
            record("arcl-t-01M5VDDD42HZQ1FEN49X8001M5", "Ends in the shared start"),
            record("arcl-t-01M5VDDD42HZQ1FEN49X80GBA5", "Other"),
        ];
        assert_eq!(
            resolve_short_id("001M5", &records, None).map(|record| record.title.as_str()),
            Ok("Ends in the shared start")
        );
        assert!(matches!(
            resolve_short_id("01M5", &records, None),
            Err(ShortIdError::Prefix { .. })
        ));
    }

    #[test]
    fn shorten_text_replaces_known_ids_only() {
        let ids = ShortIds::new(records().into_iter().map(|record| record.id));
        assert_eq!(
            ids.shorten_text(
                "ancestor `arcl-t-01M5VDDD42HZQ1FEN49X80GBA5` is blocked by `arcl-t-01M5VDDD42HZQ1FEN49X80535B` (pending), not arcl-n-01M5VDDD42HZQ1FEN49X80ZZZZ"
            ),
            "ancestor `GBA5` is blocked by `0535B` (pending), not arcl-n-01M5VDDD42HZQ1FEN49X80ZZZZ"
        );
        assert_eq!(
            ShortIds::default().shorten_text("`arcl-t-01M5VDDD42HZQ1FEN49X80GBA5`"),
            "`arcl-t-01M5VDDD42HZQ1FEN49X80GBA5`"
        );
    }

    #[test]
    fn a_full_id_prefix_matches_in_any_case() {
        assert!(is_full_id("arcl-t-01M5VDDD42HZQ1FEN49X80GBA5"));
        assert!(is_full_id("ARCL-T-01M5VDDD42HZQ1FEN49X80GBA5"));
        assert!(!is_full_id("t-GBA5"));
        assert!(!is_full_id("arc"));
    }
}
