use super::Ending;
use af_wire::authoring::Outcome;
use zeroize::Zeroize as _;

#[test]
fn a_finished_answer_is_guarded_before_accounting_and_during_handoff() {
    let mut ending = Ending::completed(Some("private-fixture-answer".to_owned()));
    ending.zeroize();
    assert_eq!(ending.answer.as_deref(), Some(""));
    assert_eq!(ending.outcome, Outcome::Completed);
}

#[test]
fn transferring_an_answer_releases_only_the_old_owner() {
    let mut ending = Ending::completed(Some("private-fixture-answer".to_owned()));
    let transferred = ending.answer.take();
    ending.zeroize();
    assert!(ending.answer.is_none());
    let mut answer = zeroize::Zeroizing::new(transferred.unwrap_or_default());
    assert_eq!(&**answer, "private-fixture-answer");
    answer.zeroize();
    assert!(answer.is_empty());
}
