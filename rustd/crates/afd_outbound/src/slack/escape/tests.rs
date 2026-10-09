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
