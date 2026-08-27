//! Heroku-style workspace names: `silent-raven-k7m2`.
//!
//! # Why a name is generated at all
//!
//! A workspace does not need a name to work — `core.workspaces.name` is
//! nullable and the identifier is the key. It needs one to be TALKED about: a
//! person picking from a list, a support conversation, a log line somebody is
//! reading at two in the morning. Making the caller supply one turns "create me
//! a workspace" into a naming decision they did not ask to make, and the Zig
//! daemon's `POST /v1/workspaces` refuses a blank name with a 400 for exactly
//! that non-reason. Signup bootstrap, meanwhile, generates one — the same
//! product answering the same question two ways.
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

use crate::Result;

/// The separator between every part.
const SEPARATOR: char = '-';

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
const ENTROPY_LEN: usize = 8 + SUFFIX_LEN;

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
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use afd_crypto::entropy::Entropy;

    use super::{SEPARATOR, SUFFIX_ALPHABET, SUFFIX_LEN, generate};

    #[test]
    fn a_generated_name_has_the_documented_shape() {
        let name = generate(&Entropy::new()).expect("a host can draw random bytes");
        let parts: Vec<&str> = name.split(SEPARATOR).collect();

        assert_eq!(
            parts.len(),
            3,
            "the shape is adjective-noun-suffix, got {name}"
        );
        assert!(
            parts.iter().all(|part| !part.is_empty()),
            "no part may be empty, got {name}"
        );
        let suffix = parts.last().expect("a three-part name has a last part");
        assert_eq!(
            suffix.len(),
            SUFFIX_LEN,
            "the suffix is a fixed width so names line up in a list, got {name}"
        );
    }

    #[test]
    fn a_name_survives_a_url_and_a_terminal_unquoted() {
        // The whole reason for a generated name is that a person reads it back
        // and types it somewhere. Every character has to be one that survives
        // that trip without escaping.
        for _draw in 0..64 {
            let name = generate(&Entropy::new()).expect("a host can draw random bytes");
            assert!(
                name.bytes().all(|byte| byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || byte == SEPARATOR as u8),
                "{name} carries a character that would need quoting"
            );
        }
    }

    #[test]
    fn the_suffix_avoids_the_characters_people_misread() {
        // `l`/`1` and `o`/`0` are the pairs somebody transcribing a name off a
        // support ticket gets wrong. This asserts the alphabet, not a sample,
        // because a sample would pass by luck.
        for ambiguous in *b"loi01" {
            assert!(
                !SUFFIX_ALPHABET.contains(&ambiguous),
                "{} is a character people mistype",
                char::from(ambiguous)
            );
        }
    }

    #[test]
    fn two_names_in_a_row_differ() {
        // Not a distribution proof — that belongs to the entropy source, which
        // has its own. This catches the specific regression of a generator that
        // draws once and reuses, which would make the unique index the only
        // thing standing between a tenant and one workspace.
        let first = generate(&Entropy::new()).expect("a host can draw random bytes");
        let second = generate(&Entropy::new()).expect("a host can draw random bytes");
        assert_ne!(
            first, second,
            "a generator that repeats turns every create after the first into a retry"
        );
    }
}
