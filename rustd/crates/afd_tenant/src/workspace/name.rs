//! Heroku-style workspace names: `silent-raven-k7m2`.
//!
//! # Why a name is generated at all
//!
//! A workspace does not need a name to work — `core.workspaces.name` is
//! nullable and the identifier is the key. It needs one to be TALKED about: a
//! person picking from a list, a support conversation, a log line somebody is
//! reading at two in the morning. Making the caller supply one turns "create me
//! a workspace" into a naming decision they did not ask to make. Signup
//! bootstrap generates one, so `POST /v1/workspaces` does too for a blank name
//! — the same product answering the same question one way.
//!
//! # Where the words come from
//!
//! `petname`'s curated English lists, not a hand-written array. The word list
//! IS the product here — it decides whether names read as friendly or as
//! nonsense — and a maintained upstream is better at it than a constant we
//! would never revisit.
//!
//! Only the lists are borrowed. Selection uses [`Entropy`], this workspace's
//! single random source, rather than `petname`'s own generator: a second
//! entropy surface is a second thing to seed, to mock in a test, and to get
//! wrong. `petname` exposes its lists as plain slices, so this costs one index
//! per word and buys back the property that all randomness in the daemon comes
//! from one place.
//!
//! # The suffix is what makes it unique enough to retry
//!
//! Two words alone collide often enough to matter at scale. The four-character
//! suffix multiplies the space by about a million, which does not make a
//! collision impossible and is not meant to: `uq_workspaces_tenant_id_name` is
//! the arbiter, and the caller retries. The suffix makes a retry rare, not
//! unnecessary.

use afd_crypto::entropy::Entropy;

use crate::{Result, error};

/// The separator between every part.
const SEPARATOR: char = '-';

/// The most Unicode code points a caller-supplied name may carry — counted
/// the way a person counts "128 characters", not the way UTF-8 spends bytes.
const MAX_NAME_CODEPOINTS: usize = 128;

/// The ASCII whitespace a name's ends lose before any rule runs.
const TRIMMED: &[char] = &[' ', '\t', '\x0b', '\x0c', '\r', '\n'];

/// A workspace name the caller chose, already past every rule.
///
/// Constructed only by [`Chosen::parse`], so a handler holding one cannot be
/// holding a control character or an over-long name — there is no validation
/// arm anywhere downstream, and none a stub could get differently right.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chosen(String);

impl Chosen {
    /// Reads a caller's name, deciding between the three outcomes.
    ///
    /// `Ok(Some)` is a name to store; `Ok(None)` says the caller chose
    /// nothing — absent, empty once trimmed, or whitespace however spelled —
    /// and the create generates one instead, rather than answering a 400.
    ///
    /// # Errors
    /// Refuses a name carrying a control character, a bidirectional override,
    /// or a line separator — each of which lets a name lie about itself in a
    /// list — and one past the code-point cap. The character check runs first,
    /// so a long name with a forbidden character is refused for the character.
    pub fn parse(raw: &str) -> Result<Option<Self>> {
        let trimmed = raw.trim_matches(TRIMMED);
        if trimmed.chars().any(is_forbidden) {
            return Err(error::workspace_name_invalid());
        }
        // Empty, or whitespace however spelled: the caller chose nothing.
        if trimmed.chars().all(is_unicode_whitespace) {
            return Ok(None);
        }
        garde::Unvalidated::new(Trimmed { name: trimmed })
            .validate()
            .map(|proved| Some(Self(proved.name.to_owned())))
            .map_err(|_report| error::workspace_name_too_long())
    }

    /// The name as it is stored and echoed.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A chosen name once its ends are trimmed, with the bound it must meet.
///
/// Built from the TRIMMED value, so the cap counts what is stored rather than
/// the spaces a caller pasted around it.
#[derive(Debug, garde::Validate)]
struct Trimmed<'a> {
    #[garde(length(chars, max = MAX_NAME_CODEPOINTS))]
    name: &'a str,
}

/// A code point no stored name may carry.
///
/// The C0 and C1 controls, the Arabic letter mark, the directional marks, the
/// Unicode line and paragraph separators, the bidirectional embeddings and
/// overrides, and the bidirectional isolates.
const fn is_forbidden(codepoint: char) -> bool {
    matches!(codepoint,
        '\u{0000}'..='\u{001f}'
        | '\u{007f}'..='\u{009f}'
        | '\u{061c}'
        | '\u{200e}'..='\u{200f}'
        | '\u{2028}'..='\u{2029}'
        | '\u{202a}'..='\u{202e}'
        | '\u{2066}'..='\u{2069}')
}

/// A code point that is whitespace without being ASCII whitespace.
///
/// `U+0085` is absent on purpose: it sits inside the C1 control range, so the
/// forbidden check above decides it first and a row here could never fire.
const fn is_unicode_whitespace(codepoint: char) -> bool {
    matches!(
        codepoint,
        '\u{2000}'..='\u{200a}' | '\u{00a0}' | '\u{1680}' | '\u{202f}' | '\u{205f}' | '\u{3000}'
    )
}

/// Characters a generated suffix is drawn from.
///
/// Lower-case letters and digits, minus `l`, `o`, `i`, `0` and `1`. Those five
/// are the pairs people mistype when they read a name off a screen and into a
/// terminal, and this string exists to be read off a screen. Dropping them
/// costs about a fifth of the space and the suffix has plenty.
const SUFFIX_ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";

/// How many characters the suffix carries.
///
/// Four of a 31-character alphabet is a little under a million, which is the
/// point at which a per-tenant collision stops being something a person would
/// ever see. A tenant with a million workspaces has other problems.
const SUFFIX_LEN: usize = 4;

/// The word a name falls back to when a list is somehow empty.
///
/// Unreachable with `default-words` compiled in. Named rather than spelled at
/// both sites so the two cannot drift into different fallbacks, which would
/// make one branch untestable against the other (RULE UFS).
const FALLBACK_WORD: &str = "workspace";

/// Bytes drawn per generated name: one per word choice, plus the suffix.
///
/// Public so a suite scripting the draw queues exactly one name's worth.
pub const ENTROPY_LEN: usize = 8 + SUFFIX_LEN;

/// Generates a name in the shape `adjective-noun-suffix`.
///
/// # Errors
/// Reports a host that cannot draw random bytes. Not degraded to a weaker
/// source — a predictable name is a guessable one, and while a workspace name
/// is not a secret, a generator that quietly stopped being random would make
/// collisions systematic rather than rare.
pub fn generate(entropy: &Entropy) -> Result<String> {
    let mut bytes = [0u8; ENTROPY_LEN];
    entropy.fill(&mut bytes)?;

    // `default()` is petname's small English lists — the ones the upstream
    // project curates. Held for the length of this call only: the lists are
    // `&'static str` slices, so this borrows rather than allocating them.
    let words = petname::Petnames::default();
    let adjective = pick(
        &words.adjectives,
        u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
    );
    let noun = pick(
        &words.nouns,
        u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
    );

    let mut name = String::with_capacity(adjective.len() + noun.len() + SUFFIX_LEN + 2);
    name.push_str(adjective);
    name.push(SEPARATOR);
    name.push_str(noun);
    name.push(SEPARATOR);
    for byte in &bytes[8..] {
        // `% len` is the modulo bias every rejection-sampling argument is
        // about. It is irrelevant here and stating why is cheaper than an
        // argument later: this picks a display name, not a key, and the
        // resulting distribution is off by a fraction of a percent on an
        // alphabet nobody is attacking.
        let index = usize::from(*byte) % SUFFIX_ALPHABET.len();
        // `get` rather than an index: the modulo makes it unreachable, and the
        // daemon's lint set does not take "unreachable" as an answer on a path
        // a panic would take the process down from.
        if let Some(character) = SUFFIX_ALPHABET.get(index) {
            name.push(char::from(*character));
        }
    }
    Ok(name)
}

/// One word from `list`, or a fallback when the list is somehow empty.
///
/// The empty case cannot happen with `default-words` compiled in, and is
/// handled rather than indexed blindly because a panic on the signup path would
/// be a denial of service reachable by a future word-list change.
fn pick<'a>(list: &'a [&'a str], draw: u32) -> &'a str {
    if list.is_empty() {
        return FALLBACK_WORD;
    }
    let index = draw as usize % list.len();
    list.get(index).copied().unwrap_or(FALLBACK_WORD)
}

#[cfg(test)]
#[path = "name/tests.rs"]
mod tests;
