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

/// `text` with every character Slack reads as markup replaced by its entity,
/// so no post notifies anyone or renders as a link it did not spell out.
pub(super) fn literal(text: &str) -> String {
    text.chars()
        .fold(String::with_capacity(text.len()), |mut out, c| {
            match SLACK_ENTITIES.iter().find(|(markup, _)| *markup == c) {
                Some((_, entity)) => out.push_str(entity),
                None => out.push(c),
            }
            out
        })
}

#[cfg(test)]
mod tests;
