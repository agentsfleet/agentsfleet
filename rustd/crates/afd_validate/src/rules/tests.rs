//! Each rule at its edge, called directly and from a derive.

use garde::Validate as _;

use super::{HAS_NUL, NOT_ASCII_DIGITS, NOT_FINITE, OUTSIDE_CHARSET};
use super::{ascii_digits, charset, finite, nul_free};

/// The rules as an author declares them: beside garde's own bounds, through
/// the attribute, on borrowed and owned text alike.
#[derive(Debug, garde::Validate)]
struct Declared {
    #[garde(custom(finite), range(min = 0.0))]
    budget: f64,
    #[garde(length(bytes, min = 1, max = 8), custom(nul_free))]
    name: String,
    #[garde(custom(charset(|c: char| c.is_ascii_lowercase() || c == '-')))]
    slug: &'static str,
}

fn declared() -> Declared {
    Declared {
        budget: 1.5,
        name: "ok".to_owned(),
        slug: "a-b",
    }
}

fn messages(value: &Declared) -> Vec<(String, String)> {
    value.validate().map_or_else(
        |report| {
            report
                .iter()
                .map(|(path, error)| (path.to_string(), error.message().to_owned()))
                .collect()
        },
        |()| Vec::new(),
    )
}

#[test]
fn test_finite_refuses_nan_and_infinity() {
    for refused in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            finite(&refused, &()).map_err(|error| error.message().to_owned()),
            Err(NOT_FINITE.to_owned()),
            "{refused} was admitted"
        );
    }
    for admitted in [0.0, -1.5, f64::MAX, f64::MIN_POSITIVE] {
        assert_eq!(finite(&admitted, &()), Ok(()), "{admitted} was refused");
    }
}

#[test]
fn a_declared_float_range_no_longer_admits_nan() {
    // The reason `finite` exists: garde's own range passes NaN through.
    let mut value = declared();
    value.budget = f64::NAN;
    assert_eq!(
        messages(&value),
        vec![("budget".to_owned(), NOT_FINITE.to_owned())]
    );
}

#[test]
fn nul_free_refuses_only_the_nul() {
    assert_eq!(nul_free("plain text, é", &()), Ok(()));
    assert_eq!(
        nul_free("a\0b", &()).map_err(|error| error.message().to_owned()),
        Err(HAS_NUL.to_owned())
    );
}

#[test]
fn ascii_digits_refuses_signs_spaces_and_other_numerals() {
    assert_eq!(ascii_digits("0123456789", &()), Ok(()));
    for refused in ["+5", "-1", " 5", "1e2", "٣", "ten"] {
        assert_eq!(
            ascii_digits(refused, &()).map_err(|error| error.message().to_owned()),
            Err(NOT_ASCII_DIGITS.to_owned()),
            "{refused:?} was admitted"
        );
    }
}

#[test]
fn a_charset_names_its_predicate_beside_the_field() {
    let mut value = declared();
    value.slug = "A_b";
    assert_eq!(
        messages(&value),
        vec![("slug".to_owned(), OUTSIDE_CHARSET.to_owned())]
    );
    assert_eq!(messages(&declared()), Vec::new());
}

#[test]
fn a_rule_and_a_length_on_one_field_both_report() {
    // garde runs every rule on a field; the rule judges content, the length
    // judges size, and neither answers for the other.
    let mut value = declared();
    value.name = "nine\0byte".to_owned();
    let reported = messages(&value);
    assert_eq!(reported.len(), 2, "{reported:?}");
    assert!(reported.iter().all(|(path, _)| path == "name"));
}
