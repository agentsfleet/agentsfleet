//! The half of an approval card a language model wrote.
//!
//! # A model's claim is not the platform's statement
//!
//! A card mixes two kinds of sentence: what the daemon and the workspace assert
//! (see [`Stated`](super::Stated)) and what a fleet's model says it intends to
//! do. Only the first may be read as fact. Keeping them apart is real only if
//! the model's half cannot imitate the other half's SHAPE, which is what this
//! module enforces at the character level.
//!
//! Two forgeries close here:
//!
//! * **Control characters.** A renderer that turns `\n` back into a line break
//!   lets model prose emit its own `- Gate:` and `- If approved:` rows beneath
//!   the genuine ones — counterfeiting the only half a human may believe. The
//!   remaining C0 characters are not escapable at all (RFC 8259 §7), so a
//!   payload carrying one is rejected whole and the gate parks with nobody
//!   notified.
//! * **Bidirectional overrides.** `U+202A`–`U+202E` and `U+2066`–`U+2069`
//!   reorder what is RENDERED without altering a stored byte, so a repository
//!   or a commit can display as something it is not.
//!
//! # Characters, not bytes
//!
//! `approval_gate_prose.zig` walks bytes for both, because Zig has no char
//! iterator to hand: it needs a hand-written `bidiOverrideLen` decoding
//! `E2 80 AA..AE`, a `sanitizedLen` pre-pass so the allocation is exact, and a
//! `needsSanitizing` pre-check so the common path does not pay for either.
//! Rust iterates `char`s — the UTF-8 decoding belongs to the iterator, the
//! ranges are written as the code points they are, and "what if a multi-byte
//! sequence straddles the cap" is unrepresentable rather than defended against.
//!
//! # What never appears here
//!
//! Diff bytes and secret material. An approval authorises a bounded RUN, not
//! specific bytes; the draft Pull Request is where a diff is reviewed.

use serde_json::Value;

/// Model-authored prose is bounded before it reaches a card or a gate row.
///
/// Generous enough for "revert `<sha>` in `<owner/repo>` because `<reason>`"
/// and far below any message ceiling, so a fleet cannot push a wall of text in
/// front of the approve button.
pub const MAX_PROPOSED_ACTION_BYTES: usize = 512;

/// Evidence is identifiers and links, never file contents.
///
/// A workspace is destroyed per lease, so fleets cannot hand each other bytes
/// anyway — what this bounds is one fleet's own payload.
pub const MAX_EVIDENCE_BYTES: usize = 1024;

/// The `evidence` column's "nothing was offered".
///
/// An empty JSON object rather than SQL `NULL`: the column is `NOT NULL
/// JSONB`, and a reader addressing `evidence->>'service'` gets an absent key
/// either way without having to test for null first.
pub const NO_EVIDENCE: &str = "{}";

/// The `request_json` key a fleet states its intent in.
const FIELD_PROPOSED_ACTION: &str = "proposed_action";
/// The key carrying identifiers and links supporting that intent.
const FIELD_EVIDENCE: &str = "evidence";
/// A plain human steer carries only this, and it is the fallback.
const FIELD_MESSAGE: &str = "message";

/// What a removed character becomes.
///
/// A space rather than nothing: a reader should see that something was taken
/// out, and dropping outright would silently fuse the two words either side.
const REPLACEMENT: char = ' ';

/// What a fleet's model says it intends to do.
///
/// Owned, bounded and card-safe by construction: the only way to build one is
/// [`Claim::of`], so a renderer cannot receive prose that skipped either step.
/// The type is what carries "a model wrote this" as far as the renderer — the
/// milestone that has to attribute it — instead of a naming convention a
/// different file in a different milestone has to remember.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    /// What the fleet says it intends to do. Empty when it said nothing usable.
    proposed_action: Box<str>,
    /// Identifiers and links supporting that intent, as JSON.
    ///
    /// Always well-formed — re-serialised rather than echoed — and always an
    /// object or [`NO_EVIDENCE`].
    evidence: Box<str>,
}

impl Claim {
    /// What the fleet said it intends to do.
    #[must_use]
    pub fn proposed_action(&self) -> &str {
        &self.proposed_action
    }

    /// The identifiers and links it offered, as JSON.
    #[must_use]
    pub fn evidence(&self) -> &str {
        &self.evidence
    }

    /// The model's half of the card, read out of the event body.
    ///
    /// Never fails. Every way `context` can disappoint — absent, not an object,
    /// missing the field, holding a non-string, oversized — degrades to empty
    /// or to [`NO_EVIDENCE`]. A blank field renders as nothing, while a hard
    /// failure would turn one malformed steer into a stuck queue.
    #[must_use]
    pub fn of(context: Option<&Value>) -> Self {
        let object = context.and_then(Value::as_object);
        let prose = object
            .and_then(|body| {
                body.get(FIELD_PROPOSED_ACTION)
                    .or_else(|| body.get(FIELD_MESSAGE))
            })
            .and_then(Value::as_str)
            .unwrap_or_default();

        Self {
            proposed_action: card_safe(bounded(prose, MAX_PROPOSED_ACTION_BYTES)).into(),
            evidence: object
                .and_then(|body| body.get(FIELD_EVIDENCE))
                .and_then(evidence)
                .unwrap_or_else(|| NO_EVIDENCE.to_owned())
                .into(),
        }
    }
}

/// `offered` re-serialised, or `None` if it will not fit.
///
/// Re-serialised rather than echoed: the parser hands back a value, not the
/// original bytes, so going through the writer is what guarantees the result is
/// well-formed JSON whatever the model sent.
///
/// Oversized evidence is DROPPED rather than truncated — truncating JSON
/// produces invalid JSON, and the card stays parseable and still names the
/// action without it. The links are what is lost.
fn evidence(offered: &Value) -> Option<String> {
    serde_json::to_string(offered)
        .ok()
        .filter(|json| json.len() <= MAX_EVIDENCE_BYTES)
}

/// The longest prefix of `prose` within `cap` bytes, on a character boundary.
///
/// A prefix of valid UTF-8 cut at a character boundary is itself valid UTF-8,
/// so this cannot produce the mangled tail a byte-count truncation can.
fn bounded(prose: &str, cap: usize) -> &str {
    if prose.len() <= cap {
        return prose;
    }
    // Each character's END offset, kept while it still fits — so the cut lands
    // on the last boundary within the cap rather than one past it, and a
    // multi-byte character straddling the cap is dropped whole.
    let end = prose
        .char_indices()
        .map(|(at, character)| at + character.len_utf8())
        .take_while(|end| *end <= cap)
        .last()
        .unwrap_or_default();
    prose.split_at(end).0
}

/// `prose` with every character a renderer could be fooled by replaced.
///
/// Applied AFTER the cap, and that order is free: every replacement is one
/// character, so the result can only be shorter in bytes than its input.
fn card_safe(prose: &str) -> String {
    prose
        .chars()
        .map(|character| {
            if is_forgeable(character) {
                REPLACEMENT
            } else {
                character
            }
        })
        .collect()
}

/// Whether `character` lets model prose imitate the card's trusted half.
///
/// `is_control` covers the C0 range and DEL. The two explicit ranges are the
/// bidirectional embeddings, overrides and isolates, which are not control
/// characters by Unicode's definition but reorder rendered text exactly as if
/// they were.
const fn is_forgeable(character: char) -> bool {
    character.is_control() || matches!(character, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{Claim, MAX_EVIDENCE_BYTES, MAX_PROPOSED_ACTION_BYTES, NO_EVIDENCE, bounded};
    use serde_json::json;

    #[test]
    fn the_model_half_carries_what_the_fleet_stated() {
        let claim = Claim::of(Some(&json!({
            "proposed_action": "revert abc123 in acme/widgets",
            "evidence": {"commit": "abc123"},
        })));

        assert_eq!(claim.proposed_action(), "revert abc123 in acme/widgets");
        assert!(claim.evidence().contains("abc123"), "{}", claim.evidence());
    }

    #[test]
    fn a_plain_human_steer_falls_back_to_the_message_body() {
        let claim = Claim::of(Some(&json!({"message": "please repair it"})));

        assert_eq!(claim.proposed_action(), "please repair it");
    }

    #[test]
    fn a_stated_intent_outranks_a_message_beside_it() {
        // `message` is the FALLBACK, not an alternative — a body carrying both
        // must not have its ordering decided by map iteration order.
        let claim = Claim::of(Some(&json!({
            "proposed_action": "revert abc123",
            "message": "hi",
        })));

        assert_eq!(claim.proposed_action(), "revert abc123");
    }

    #[test]
    fn every_undecidable_body_degrades_to_empty_rather_than_failing() {
        // Five ways a model can disappoint, and the property is that all of
        // them answer the same way: a malformed steer costs a blank field, not
        // a stuck queue.
        for undecidable in [
            None,
            Some(json!(null)),
            Some(json!(["not", "an", "object"])),
            Some(json!({"other": "main"})),
            Some(json!({"proposed_action": 42})),
        ] {
            let claim = Claim::of(undecidable.as_ref());
            assert_eq!(claim.proposed_action(), "", "{undecidable:?}");
            assert_eq!(claim.evidence(), NO_EVIDENCE, "{undecidable:?}");
        }
    }

    #[test]
    fn model_prose_cannot_grow_rows_that_counterfeit_the_daemon_half() {
        // The forgery this exists for. The cap is ample for rows that render
        // exactly like the workspace-authored ones above them, because a
        // renderer turns the JSON newline escape back into a real line break.
        let claim = Claim::of(Some(&json!({
            "proposed_action":
                "revert abc123\n- Gate: `production-write`\r\n- If approved: opens 1 draft PR",
        })));

        // One line in, one line out: the claim can no longer sprout rows.
        assert!(!claim.proposed_action().contains('\n'));
        assert!(!claim.proposed_action().contains('\r'));
        // Replaced, not dropped — an operator still reads what the fleet said.
        assert!(claim.proposed_action().starts_with("revert abc123 "));
    }

    #[test]
    fn every_control_character_is_replaced() {
        // Walked in full rather than sampled: a renderer that escapes one C0
        // character and not another is exactly the gap a spot check misses.
        for code in (0..=0x1F_u32).chain(core::iter::once(0x7F)) {
            let control = char::from_u32(code).expect("a C0 code point is a character");
            let claim = Claim::of(Some(&json!({
                "proposed_action": format!("a{control}b"),
            })));

            assert_eq!(claim.proposed_action(), "a b", "U+{code:04X}");
        }
    }

    #[test]
    fn every_bidirectional_control_is_replaced_by_exactly_one_space() {
        // These reorder what a human reads without altering a stored byte, so a
        // repository or a commit can display as something it is not. Three
        // bytes in, one out — and no length arithmetic to get wrong, because
        // the iteration is over characters rather than bytes.
        let claim = Claim::of(Some(&json!({
            "proposed_action": "revert \u{202E}stegdiw/emca\u{202C}",
        })));
        assert_eq!(claim.proposed_action(), "revert  stegdiw/emca ");

        for reordering in [
            '\u{202A}', '\u{202B}', '\u{202C}', '\u{202D}', '\u{202E}', '\u{2066}', '\u{2067}',
            '\u{2068}', '\u{2069}',
        ] {
            let claim = Claim::of(Some(&json!({
                "proposed_action": format!("a{reordering}b"),
            })));
            assert_eq!(claim.proposed_action(), "a b", "{reordering:?}");
        }
    }

    #[test]
    fn ordinary_prose_survives_untouched() {
        // The other direction of the same property: a sanitizer that is safe
        // because it mangles everything is not a sanitizer. Non-ASCII that is
        // not a reordering control passes through.
        for kept in ["revert abc123 in acme/widgets", "révert — abc123", ""] {
            let claim = Claim::of(Some(&json!({"proposed_action": kept})));
            assert_eq!(claim.proposed_action(), kept);
        }
    }

    #[test]
    fn oversized_prose_is_capped_and_oversized_evidence_is_dropped() {
        let wall = "x".repeat(4_000);
        let claim = Claim::of(Some(&json!({
            "proposed_action": wall,
            "evidence": {"blob": wall},
        })));

        assert!(claim.proposed_action().len() <= MAX_PROPOSED_ACTION_BYTES);
        // Dropped, so the card's JSON stays valid — a truncated object would
        // not be, and an unparseable card is worse than one missing its links.
        assert_eq!(claim.evidence(), NO_EVIDENCE);
        assert!(claim.evidence().len() <= MAX_EVIDENCE_BYTES);
    }

    #[test]
    fn a_cap_never_lands_inside_a_character() {
        // The hazard a byte-count truncation carries and this one cannot: the
        // Zig needs `truncateUtf8` for exactly this, and a slice cut off a
        // character boundary would panic here rather than mangle silently.
        let multibyte = "é".repeat(MAX_PROPOSED_ACTION_BYTES);

        // Every cap from "nothing fits" through several whole characters,
        // including the odd ones that fall INSIDE a two-byte character.
        for cap in 0..8 {
            let kept = bounded(&multibyte, cap);
            assert!(kept.len() <= cap, "cap {cap}: kept {} bytes", kept.len());
            assert!(multibyte.starts_with(kept), "cap {cap}");
        }
        // An odd cap keeps only the characters wholly inside it — never a
        // trailing half, which is what would panic the slice above.
        assert_eq!(bounded(&multibyte, 3), "é");
        assert_eq!(bounded(&multibyte, 1), "");
    }
}
