use super::Ending;
use af_wire::authoring::Outcome;

#[test]
fn a_finished_answer_is_held_by_a_wiping_owner() {
    let ending = Ending::completed(Some("private-fixture-answer".to_owned()));
    // The annotation fails to compile if the field ever loses its wiping wrapper.
    let answer: Option<&zeroize::Zeroizing<String>> = ending.answer.as_ref();
    assert_eq!(
        answer.map(|answer| answer.as_str()),
        Some("private-fixture-answer")
    );
    assert_eq!(ending.outcome, Outcome::Completed);
}

#[test]
fn handing_an_answer_to_its_report_moves_the_buffer_without_a_copy() {
    let mut ending = Ending::completed(Some("private-fixture-answer".to_owned()));
    let buffer = ending.answer.as_ref().map(|answer| answer.as_ptr());
    let handed = ending.take_answer();
    assert!(ending.answer.is_none(), "the run keeps no second owner");
    assert_eq!(handed.as_deref(), Some("private-fixture-answer"));
    assert_eq!(handed.as_ref().map(|answer| answer.as_ptr()), buffer);
}
