//! What a request's statement answered, mapped to what the run does next, and
//! the card a person reads.
//!
//! Nothing here needs a datastore: the mapping is a pure function of the row
//! the statement returned, and the card copy of the wanted grant.

use af_core::id::Uuid7;
use afd_api_wire::grant::kind;

use super::ask::RequestRow;
use super::{GrantKind, KIND_INTEGRATION_GRANT, Requested, Wanted};

type Outcome = Result<(), Box<dyn std::error::Error>>;

fn id(text: &str) -> Result<Uuid7, Box<dyn std::error::Error>> {
    Ok(Uuid7::parse(text)?)
}

const GRANT: &str = "01890a5d-ac96-774b-bcce-b302099a8057";
const CARD: &str = "01890a5d-ac96-774b-bcce-b302099a8058";
const OPEN: &str = "01890a5d-ac96-774b-bcce-b302099a8059";

fn asked(
    asked: bool,
    raised: Option<&str>,
    open: Option<&str>,
) -> Result<RequestRow, Box<dyn std::error::Error>> {
    Ok(RequestRow {
        grant_id: id(GRANT)?,
        asked,
        raised: raised.map(id).transpose()?,
        open: open.map(id).transpose()?,
    })
}

#[test]
fn a_raised_card_is_the_answer_whatever_else_was_open() -> Outcome {
    let expected = Requested::Raised {
        grant: id(GRANT)?,
        gate: id(CARD)?,
    };
    assert_eq!(asked(true, Some(CARD), None)?.outcome(), expected);
    Ok(())
}

#[test]
fn an_open_card_stands_and_none_is_raised_beside_it() -> Outcome {
    let expected = Requested::Pending {
        grant: id(GRANT)?,
        gate: Some(id(OPEN)?),
    };
    assert_eq!(asked(true, None, Some(OPEN))?.outcome(), expected);
    assert_eq!(asked(false, None, Some(OPEN))?.outcome(), expected);
    Ok(())
}

/// A request that asked and saw no card lost a race to one it could not see:
/// it waits on that card rather than reporting the grant covered.
#[test]
fn an_ask_that_raised_nothing_waits_rather_than_continuing() -> Outcome {
    let outcome = asked(true, None, None)?.outcome();
    assert_eq!(
        outcome,
        Requested::Pending {
            grant: id(GRANT)?,
            gate: None
        }
    );
    assert_eq!(outcome.gate(), None);
    Ok(())
}

#[test]
fn a_request_that_asked_nothing_is_covered() -> Outcome {
    let outcome = asked(false, None, None)?.outcome();
    assert_eq!(outcome, Requested::Covered { grant: id(GRANT)? });
    assert_eq!(outcome.grant(), &id(GRANT)?);
    assert_eq!(outcome.gate(), None);
    Ok(())
}

#[test]
fn evidence_carries_the_name_the_approve_statement_joins_on_and_the_scope() -> Outcome {
    let turn = id(GRANT)?;
    let scope = r#"{"repositories":["org/app"],"access":"read"}"#;
    let evidence: serde_json::Value = serde_json::from_str(
        &Wanted {
            name: "github",
            kind: GrantKind::Integration,
            scope: Some(scope),
            turn: &turn,
        }
        .evidence(),
    )?;
    assert_eq!(
        evidence,
        serde_json::json!({
            "service": "github",
            "kind": "integration",
            "scope": {"repositories": ["org/app"], "access": "read"},
        })
    );
    Ok(())
}

#[test]
fn a_secret_request_carries_no_scope() -> Outcome {
    let turn = id(GRANT)?;
    let evidence: serde_json::Value = serde_json::from_str(
        &Wanted {
            name: "sentry",
            kind: GrantKind::Secret,
            scope: None,
            turn: &turn,
        }
        .evidence(),
    )?;
    assert_eq!(evidence.get("scope"), Some(&serde_json::Value::Null));
    assert_eq!(evidence.get("service"), Some(&serde_json::json!("sentry")));
    Ok(())
}

#[test]
fn each_kind_spells_the_column_and_names_its_channel_on_the_card() {
    assert_eq!(GrantKind::Secret.as_str(), kind::SECRET);
    assert_eq!(GrantKind::Integration.as_str(), kind::INTEGRATION);
    let (secret, _) = GrantKind::Secret.card("sentry");
    let (integration, _) = GrantKind::Integration.card("github");
    assert_eq!(secret, "use the workspace secret sentry");
    assert_eq!(integration, "mint short-lived credentials for github");
}

#[test]
fn the_card_type_is_the_one_the_resolve_binds() {
    assert_eq!(KIND_INTEGRATION_GRANT, "integration_grant");
}
