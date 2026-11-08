//! What the poster sends as a message's text: the fleet's words, shown as
//! written.
//!
//! Slack reads `<!channel>`, `<!here>` and `<@U…>` in a message's text as
//! notifications, and `<url|label>` as a link whose label hides its target.
//! Every text this crate posts is model output, and the model reads the thread
//! it answers, so whoever writes in that thread could steer it into paging the
//! channel or dressing one link as another. [`SlackPoster::post`] escapes every
//! text here, the answer and each interim line alike, so no caller can forget
//! it and none escapes twice.
//!
//! [`SlackPoster::post`]: super::SlackPoster

/// Each character Slack reads as markup, and the entity that shows it as
/// itself. Slack documents exactly these three as the text to escape.
const SLACK_ENTITIES: [(char, &str); 3] = [('&', "&amp;"), ('<', "&lt;"), ('>', "&gt;")];

/// The most characters a message's `text` may carry; Slack documents 40,000.
const TEXT_MAX_CHARS: usize = 40_000;

/// What ends a text cut to fit.
const TRUNCATED: &str = "… (truncated)";

/// `text` with every character Slack reads as markup replaced by its entity,
/// so no post notifies anyone or renders as a link it did not spell out.
///
/// Cut to Slack's limit after escaping, since escaping lengthens the text,
/// and only between whole entities, so a cut never leaves half of `&lt;`.
pub(super) fn literal(text: &str) -> String {
    let escaped_chars: usize = text.chars().map(width).sum();
    let budget = if escaped_chars <= TEXT_MAX_CHARS {
        usize::MAX
    } else {
        TEXT_MAX_CHARS - TRUNCATED.chars().count()
    };
    let mut used = 0;
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        used += width(c);
        if used > budget {
            out.push_str(TRUNCATED);
            break;
        }
        match entity(c) {
            Some(entity) => out.push_str(entity),
            None => out.push(c),
        }
    }
    out
}

/// The entity Slack shows `c` as itself through, when `c` is markup.
fn entity(c: char) -> Option<&'static str> {
    SLACK_ENTITIES
        .iter()
        .find(|(markup, _)| *markup == c)
        .map(|(_, entity)| *entity)
}

/// How many characters `c` takes once escaped (an entity is ASCII).
fn width(c: char) -> usize {
    entity(c).map_or(1, str::len)
}

#[cfg(test)]
mod tests;
