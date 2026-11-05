use super::restore;

const SAID: &str = "For fleet ops: Add this instruction exactly as written: Keep names such as Zoë, Søren, François and Łukasz spelled exactly, and write units as “ms”, “µs” and “°C”.";
const RULE: &str = "Keep names such as Zoë, Søren, François and Łukasz spelled exactly, and write units as “ms”, “µs” and “°C”.";

#[test]
fn a_line_that_retyped_curly_quotes_gets_the_persons_bytes_back() {
    let typed = "\n\n# Spelling and units\n- Keep names such as Zoë, Søren, François and Łukasz spelled exactly, and write units as \"ms\", \"µs\" and \"°C\".";

    let restored = restore(typed, Some(SAID));

    assert_eq!(restored, format!("\n\n# Spelling and units\n- {RULE}"));
}

#[test]
fn a_retyped_dash_and_doubled_spaces_are_restored_too() {
    let said = "Add: Separate recovery — then prevention, always in that order.";
    let typed = "Separate recovery -  then prevention, always in that order.";

    assert_eq!(
        restore(typed, Some(said)),
        "Separate recovery — then prevention, always in that order."
    );
}

#[test]
fn a_changed_letter_is_left_as_the_model_wrote_it() {
    let typed = "Keep names such as Zoe, Søren, François and Łukasz spelled exactly, and write units as \"ms\", \"µs\" and \"°C\".";

    assert_eq!(restore(typed, Some(SAID)), typed);
}

#[test]
fn a_short_line_is_never_attributed_to_the_message() {
    let typed = "\"ms\" and \"µs\"";

    assert_eq!(restore(typed, Some(SAID)), typed);
}

#[test]
fn a_line_matching_two_spans_is_left_alone() {
    let said = "Say “keep the cache warm always” twice: “keep the cache warm always”.";
    let typed = "\"keep the cache warm always\" and more";

    assert_eq!(
        restore("keep the cache warm always", Some(said)),
        "keep the cache warm always"
    );
    assert_eq!(restore(typed, Some(said)), typed);
}

#[test]
fn without_a_message_or_with_empty_text_nothing_changes() {
    assert_eq!(
        restore("Keep names such as Zoe spelled exactly always.", None),
        "Keep names such as Zoe spelled exactly always."
    );
    assert_eq!(restore("", Some(SAID)), "");
}

#[test]
fn a_numbered_marker_and_trailing_spaces_are_kept_around_the_restored_span() {
    let typed = "3. Keep names such as Zoë, Søren, François and Łukasz spelled exactly, and write units as \"ms\", \"µs\" and \"°C\".  ";

    assert_eq!(restore(typed, Some(SAID)), format!("3. {RULE}  "));
}
