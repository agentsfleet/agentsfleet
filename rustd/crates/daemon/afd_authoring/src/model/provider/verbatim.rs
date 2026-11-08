//! Restores the person's own wording where the model retyped it.
//!
//! Asked to add an instruction "exactly as written", a model often copies the
//! words but retypes the punctuation: curly quotes become straight ones, an
//! em dash becomes a hyphen. The daemon holds the person's message, so it does
//! not have to trust the copy. A line of model text that matches one span of
//! that message up to those typographic differences is replaced by the span's
//! original bytes. Anything else, including a changed letter, stays as the
//! model wrote it.

/// Shorter lines are too common to attribute to one span of the message.
const MIN_RESTORED_CHARS: usize = 24;

/// `text` with every line that retypes one span of `said` replaced by that span.
pub(super) fn restore(text: &str, said: Option<&str>) -> String {
    let Some(said) = said else {
        return text.to_owned();
    };
    let source = Folded::of(said);
    text.split('\n')
        .map(|line| restore_line(line, said, &source))
        .collect::<Vec<_>>()
        .join("\n")
}

fn restore_line(line: &str, said: &str, source: &Folded) -> String {
    let (lead, content) = line.split_at(list_marker(line));
    let trimmed = content.trim_end();
    if trimmed.chars().count() < MIN_RESTORED_CHARS {
        return line.to_owned();
    }
    let wanted = Folded::of(trimmed).text;
    let mut found = source.text.match_indices(&wanted).map(|(start, _)| start);
    let (Some(start), None) = (found.next(), found.next()) else {
        return line.to_owned();
    };
    let original = source
        .origin
        .get(start)
        .zip(source.origin.get(start + wanted.len()))
        .and_then(|(&from, &to)| said.get(from..to))
        .unwrap_or(trimmed);
    let tail = content.get(trimmed.len()..).unwrap_or_default();
    format!("{lead}{}{tail}", original.trim_end())
}

/// Bytes of a leading list marker the model may have added.
fn list_marker(line: &str) -> usize {
    let body = line.trim_start();
    let digits = body.bytes().take_while(u8::is_ascii_digit).count();
    let marker = if body.starts_with("- ") || body.starts_with("* ") {
        2
    } else if digits > 0
        && body
            .get(digits..)
            .is_some_and(|rest| rest.starts_with(". "))
    {
        digits + 2
    } else {
        0
    };
    line.len() - body.len() + marker
}

/// Text folded for comparison, mapping each folded byte to its original byte.
struct Folded {
    text: String,
    /// `origin[i]` is where folded byte `i` came from; one extra entry holds
    /// the end, so any folded range maps to an original range.
    origin: Vec<usize>,
}

impl Folded {
    fn of(text: &str) -> Self {
        let text_end = text.trim_end();
        let mut folded = Self {
            text: String::new(),
            origin: Vec::new(),
        };
        let mut spacing = false;
        for (offset, character) in text_end.char_indices() {
            let character = if character.is_whitespace() {
                if spacing || folded.text.is_empty() {
                    continue;
                }
                ' '
            } else {
                fold(character)
            };
            spacing = character == ' ';
            folded.text.push(character);
            folded
                .origin
                .extend(std::iter::repeat_n(offset, character.len_utf8()));
        }
        folded.origin.push(text_end.len());
        folded
    }
}

/// Typographic variants a model retypes, folded to one character.
const fn fold(character: char) -> char {
    match character {
        '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' | '\u{2032}' => '\'',
        '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' | '\u{2033}' => '"',
        '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2212}' => '-',
        other => other,
    }
}

#[cfg(test)]
#[path = "tests/verbatim.rs"]
mod tests;
