//! A scripted model renders the turn it is on and never its script.

use super::{FakeModel, say};

#[test]
fn a_scripted_model_renders_its_next_turn_and_not_its_script() {
    let (model, transcript) = FakeModel::new(vec![vec![say("scripted answer text")]]);

    let rendered = format!("{model:?}");

    assert!(
        rendered.starts_with("FakeModel { script: Script { next: 0, .. }"),
        "the next turn is shown and the script elided: {rendered}"
    );
    assert!(
        !rendered.contains("scripted answer text"),
        "the script's chunks stay out of the rendering: {rendered}"
    );
    assert!(transcript.asked().is_empty(), "nothing was asked yet");
}
