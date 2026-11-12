//! How an optional knob's value is read.
use super::knobs::API_URL_KNOB;
use super::{Fault, optional, read};
use af_core::env::MapEnv;

/// An exported-but-blank knob is unset, not a value.
///
/// The distinction is not cosmetic: a blank that read as configured would
/// be sent upstream as a bare `Bearer `, which the vendor refuses with a
/// sentence naming nothing an operator can act on. Whitespace is trimmed
/// for the same reason — a knob set from a shell heredoc arrives with a
/// newline attached.
#[test]
fn a_blank_or_whitespace_knob_is_absent_rather_than_empty() {
    let source = MapEnv::from_pairs([
        ("SET", "value"),
        ("BLANK", ""),
        ("SPACES", "   "),
        ("PADDED", "  value  "),
    ]);

    assert_eq!(optional(&source, "SET").as_deref(), Some("value"));
    assert_eq!(
        optional(&source, "BLANK"),
        None,
        "an exported empty string meant unset"
    );
    assert_eq!(
        optional(&source, "SPACES"),
        None,
        "whitespace is not a value"
    );
    assert_eq!(
        optional(&source, "PADDED").as_deref(),
        Some("value"),
        "a knob set from a heredoc arrives padded and is still that value"
    );
    assert_eq!(optional(&source, "ABSENT"), None);
}

/// The scheduler calls every fire back under the API url, so one that cannot
/// form that callback refuses boot instead of registering schedules that never
/// fire.
#[test]
fn an_api_url_that_cannot_form_a_callback_refuses_boot() {
    for raw in [
        "not a url",
        "https://api.example.test/v2?region=eu",
        "https://api.example.test/#top",
    ] {
        let mut faults = Vec::new();

        let kept = read::deployment(raw.into(), &mut faults);

        assert_eq!(&*kept, raw);
        assert!(
            matches!(
                faults.as_slice(),
                [Fault::Invalid {
                    knob: API_URL_KNOB,
                    ..
                }]
            ),
            "{raw}: {faults:?}"
        );
    }
}

#[test]
fn a_plain_api_origin_forms_a_callback_and_is_kept() {
    let mut faults = Vec::new();

    let kept = read::deployment("https://api.example.test".into(), &mut faults);

    assert_eq!(&*kept, "https://api.example.test");
    assert!(faults.is_empty(), "{faults:?}");
}
