#![expect(
    clippy::expect_used,
    clippy::assertions_on_result_states,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]
use super::{
    CredentialName, FleetName, MAX_CREDENTIAL_LEN, MAX_NAME_LEN, REASON_CREDENTIAL_CHARSET,
    REASON_EMPTY, REASON_NAME_CHARSET, REASON_TOO_LONG, Version,
};

#[test]
fn a_kebab_slug_is_a_fleet_name() {
    assert_eq!(
        FleetName::parse("lead-hunter-7")
            .expect("a kebab slug is a name")
            .as_str(),
        "lead-hunter-7"
    );
}

#[test]
fn an_upper_case_name_is_refused() {
    assert!(
        FleetName::parse("Lead-Hunter").is_err(),
        "one spelling of canonical, and it is lower case"
    );
}

#[test]
fn a_name_at_the_bound_is_accepted_and_one_past_it_is_not() {
    let at_bound = "a".repeat(MAX_NAME_LEN);
    let past_bound = "a".repeat(MAX_NAME_LEN + 1);

    assert!(
        FleetName::parse(&at_bound).is_ok(),
        "the bound is inclusive"
    );
    assert!(FleetName::parse(&past_bound).is_err());
}

#[test]
fn an_empty_name_is_refused() {
    assert!(FleetName::parse("").is_err());
}

#[test]
fn a_credential_reference_admits_underscores_and_refuses_dashes() {
    assert!(CredentialName::parse("GITHUB_TOKEN_1").is_ok());
    assert!(
        CredentialName::parse("github-token").is_err(),
        "the vault row name is built from this, so the charset is closed"
    );
}

#[test]
fn a_credential_reference_at_the_bound_is_accepted() {
    let at_bound = "a".repeat(MAX_CREDENTIAL_LEN);

    assert!(CredentialName::parse(&at_bound).is_ok());
    assert!(CredentialName::parse(&format!("{at_bound}b")).is_err());
}

#[test]
fn a_three_part_version_is_a_version() {
    assert_eq!(
        Version::parse("1.0.1").expect("a semver triple").as_str(),
        "1.0.1"
    );
}

#[test]
fn a_zero_part_is_allowed_but_a_leading_zero_is_not() {
    assert!(Version::parse("0.1.0").is_ok(), "`0` is a legitimate part");
    assert!(
        Version::parse("01.1.0").is_err(),
        "`01` and `1` would order differently as strings"
    );
}

#[test]
fn a_version_of_the_wrong_arity_is_refused() {
    assert!(Version::parse("1.0").is_err(), "two parts is not a triple");
    assert!(
        Version::parse("1.0.0.1").is_err(),
        "four parts is not a triple"
    );
}

#[test]
fn a_prerelease_suffix_is_refused_until_a_consumer_ranks_it() {
    assert!(Version::parse("1.0.0-alpha").is_err());
}

#[test]
fn an_empty_part_is_refused() {
    assert!(Version::parse("1..0").is_err());
    assert!(Version::parse(".1.0").is_err());
}

#[test]
fn validated_names_and_versions_display_as_authored() {
    let fleet = FleetName::parse("reviewer").expect("fleet name is valid");
    let credential = CredentialName::parse("GITHUB_TOKEN").expect("credential name is valid");
    let version = Version::parse("1.2.3").expect("version is valid");

    assert_eq!(fleet.to_string(), "reviewer");
    assert_eq!(credential.to_string(), "GITHUB_TOKEN");
    assert_eq!(version.to_string(), "1.2.3");
}

#[test]
fn an_empty_credential_reference_is_refused() {
    assert!(CredentialName::parse("").is_err());
}

#[test]
fn a_refused_fleet_name_says_which_rule_it_broke() {
    let empty = FleetName::parse("").expect_err("empty is refused");
    let too_long =
        FleetName::parse(&"a".repeat(MAX_NAME_LEN + 1)).expect_err("past the bound is refused");
    let charset = FleetName::parse("Lead-Hunter").expect_err("upper case is refused");

    assert!(
        empty.to_string().ends_with(REASON_EMPTY),
        "the author is told which rule to fix, not merely that one broke: {empty}"
    );
    assert!(
        too_long.to_string().ends_with(REASON_TOO_LONG),
        "{too_long}"
    );
    assert!(
        charset.to_string().ends_with(REASON_NAME_CHARSET),
        "{charset}"
    );
}

#[test]
fn a_refused_credential_reference_says_its_own_charset_rule() {
    let charset = CredentialName::parse("github-token").expect_err("dashes are refused");

    assert!(
        charset.to_string().ends_with(REASON_CREDENTIAL_CHARSET),
        "a reference is refused by the reference charset, not the name one: {charset}"
    );
}

#[test]
fn the_length_rule_is_asked_before_the_charset_rule() {
    let long_and_mis_spelled = "A".repeat(MAX_NAME_LEN + 1);
    let long_and_dashed = "-".repeat(MAX_CREDENTIAL_LEN + 1);

    let name = FleetName::parse(&long_and_mis_spelled).expect_err("it breaks two rules");
    let reference = CredentialName::parse(&long_and_dashed).expect_err("it breaks two rules");

    assert!(
        name.to_string().ends_with(REASON_TOO_LONG),
        "length is asked first, so the author shortens before re-spelling: {name}"
    );
    assert!(
        reference.to_string().ends_with(REASON_TOO_LONG),
        "both callers ask the rules in the same order: {reference}"
    );
}
