//! Short record IDs: the end of a record's ULID, long enough to name one
//! record.
//!
//! A ULID's first 10 characters are a timestamp, so records made together
//! share a long prefix. The last 16 are random, so a suffix tells records
//! apart. A short ID is such a suffix, optionally after the record's kind, as
//! in `535B` or `t-535B`.

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
const KINDS: [&str; 7] = ["c", "r", "s", "pl", "ph", "t", "n"];

/// The Crockford base32 alphabet ULIDs use.
const CROCKFORD: &str = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Why a short ID names no record, or more than one.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ShortIdError {
    #[error("short ID `{input}` is too short; give at least {MIN_SHORT_ID_LEN} characters from the end of the ID")]
    TooShort { input: String },
    #[error("`{input}` is not a record ID; give a full ID or the end of one, such as `535B` or `t-535B`")]
    Invalid { input: String },
    #[error("short ID `{input}` names unknown kind `{kind}`; use c, r, s, pl, ph, t or n")]
    UnknownKind { input: String, kind: String },
    #[error("no record matches short ID `{input}`")]
    NotFound { input: String },
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
/// The default value knows no records and shows every ID in full.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ShortIds {
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
        let common = |left: &str, right: &str| left.bytes().zip(right.bytes()).take_while(|(a, b)| a == b).count();
        let mut lengths = HashMap::with_capacity(reversed.len());
        for (index, (key, id)) in reversed.iter().enumerate() {
            let before = index
                .checked_sub(1)
                .map_or(0, |previous| common(key, &reversed[previous].0));
            let after = reversed.get(index + 1).map_or(0, |next| common(key, &next.0));
            let length = (before.max(after) + 1).max(MIN_SHORT_ID_LEN).min(key.len());
            lengths.insert(id.clone(), length);
        }
        Self { lengths }
    }

    /// The short ID for `id`, or `id` itself when it is not a known record.
    pub fn short<'a>(&self, id: &'a str) -> &'a str {
        match self.lengths.get(id) {
            Some(&length) => {
                let ulid = ulid_part(id);
                &ulid[ulid.len().saturating_sub(length)..]
            }
            None => id,
        }
    }
}

/// Whether `input` is written as a full record ID, `arcl-<kind>-<ULID>`,
/// rather than a short ID. A full ID is parsed as it is, never resolved.
pub fn is_full_id(input: &str) -> bool {
    input.starts_with(FULL_ID_PREFIX)
}

/// Find the one record whose ID ends in the short ID `input`.
///
/// `input` is the end of a ULID, at least [`MIN_SHORT_ID_LEN`] characters,
/// optionally after a kind and a dash: `535B` or `t-535B`. Letters match in
/// either case, and `I`, `L` and `O` read as `1`, `1` and `0`, as Crockford
/// base32 decodes them.
pub fn resolve_short_id<'a>(input: &str, records: &'a [RecordRef]) -> Result<&'a RecordRef, ShortIdError> {
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
    let matches = records
        .iter()
        .filter(|record| ulid_part(&record.id).ends_with(&suffix))
        .filter(|record| kind.is_none_or(|kind| record.id.starts_with(&format!("{FULL_ID_PREFIX}{kind}-"))))
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

#[cfg(test)]
mod tests {
    use super::{RecordRef, ShortIdError, ShortIds, resolve_short_id};

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
            resolve_short_id("GBA5", &records).map(|record| record.title.as_str()),
            Ok("Second task")
        );
        assert_eq!(
            resolve_short_id("gba5", &records).map(|record| record.title.as_str()),
            Ok("Second task")
        );
        assert_eq!(
            resolve_short_id("0535b", &records).map(|record| record.title.as_str()),
            Ok("First task")
        );
        assert_eq!(
            resolve_short_id("o535b", &records).map(|record| record.title.as_str()),
            Ok("First task")
        );
    }

    #[test]
    fn an_ambiguous_suffix_lists_every_match() {
        let records = records();
        let error = resolve_short_id("535B", &records).expect_err("two records end in 535B");
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
            resolve_short_id("t-535B", &records).map(|record| record.title.as_str()),
            Ok("First task")
        );
        assert_eq!(
            resolve_short_id("S-535b", &records).map(|record| record.title.as_str()),
            Ok("A spec")
        );
        assert!(matches!(
            resolve_short_id("pl-535B", &records),
            Err(ShortIdError::NotFound { .. })
        ));
        assert!(matches!(
            resolve_short_id("x-535B", &records),
            Err(ShortIdError::UnknownKind { .. })
        ));
    }

    #[test]
    fn short_and_invalid_input_is_refused() {
        let records = records();
        assert!(matches!(
            resolve_short_id("BA5", &records),
            Err(ShortIdError::TooShort { .. })
        ));
        assert!(matches!(
            resolve_short_id("t-", &records),
            Err(ShortIdError::TooShort { .. })
        ));
        assert!(matches!(
            resolve_short_id("GBU5", &records),
            Err(ShortIdError::Invalid { .. })
        ));
        assert!(matches!(
            resolve_short_id("ZZZZ", &records),
            Err(ShortIdError::NotFound { .. })
        ));
    }

    #[test]
    fn a_whole_ulid_resolves_as_a_suffix() {
        let records = records();
        let found = resolve_short_id("01M5VDDD42HZQ1FEN49X80GBA5", &records).expect("whole ULID matches");
        assert_eq!(found.id, "arcl-t-01M5VDDD42HZQ1FEN49X80GBA5");
    }
}
