//! What a post looks like once it is literal Slack text.

use super::literal;

#[test]
fn test_a_line_naming_the_channel_notifies_nobody() {
    assert_eq!(
        literal("<!channel> deploy is down, ping <@U024BE7LH>"),
        "&lt;!channel&gt; deploy is down, ping &lt;@U024BE7LH&gt;"
    );
}

#[test]
fn test_an_ampersand_is_escaped_once() {
    assert_eq!(literal("R&D <b>"), "R&amp;D &lt;b&gt;");
    assert_eq!(
        literal("&lt;"),
        "&amp;lt;",
        "an entity the fleet typed shows as typed"
    );
}

#[test]
fn test_a_line_without_markup_is_unchanged() {
    let line = "fix pushed as a draft; «secret:github.token» masked";
    assert_eq!(literal(line), line);
}

#[test]
fn test_an_answer_past_slack_s_limit_is_cut_between_whole_entities() {
    // 13,334 `<` escape to 53,336 characters, past Slack's 40,000.
    let posted = literal(&"<".repeat(13_334));

    let length = posted.chars().count();
    assert!(
        length <= super::TEXT_MAX_CHARS,
        "{length} is past the limit"
    );
    assert!(
        length > super::TEXT_MAX_CHARS - "&lt;".len(),
        "{length} stops short of the last whole entity that fits"
    );
    assert!(
        posted.ends_with(&format!("&lt;{}", super::TRUNCATED)),
        "{posted:.40}"
    );
    let kept = posted.trim_end_matches(super::TRUNCATED);
    assert_eq!(kept.replace("&lt;", ""), "", "no entity was cut in half");
}

#[test]
fn test_an_answer_at_the_limit_is_not_cut() {
    let exact = "a".repeat(super::TEXT_MAX_CHARS);
    assert_eq!(literal(&exact), exact);
}
