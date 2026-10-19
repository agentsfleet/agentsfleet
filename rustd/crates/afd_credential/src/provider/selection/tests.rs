//! The selection and platform-default rows, decoded without a database.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]
use super::{OwnModel, PlatformDefaultRow, Selection, SelectionRow};
use afd_billing::Posture;

/// A canonical uuidv7, so a pointer parses.
const POINTER: &str = "0195b4ba-8d3a-7f13-8abc-cd0000000002";

/// A canonical uuidv7, for the admin workspace.
const WORKSPACE: &str = "0199a6f0-1c2d-7e3f-8a4b-5c6d7e8f9a0b";

fn pointed() -> SelectionRow {
    SelectionRow {
        tenant_model_id: Some(POINTER.to_owned()),
        model_name: Some("claude-opus-5".to_owned()),
        secret_ref: Some("anthropic-prod".to_owned()),
        provider: Some("anthropic".to_owned()),
        context_cap_tokens: 200_000,
    }
}

#[test]
fn a_null_pointer_is_the_platform_posture_whatever_the_joins_say() {
    let row = SelectionRow {
        tenant_model_id: None,
        ..pointed()
    };
    let selection = row.into_selection().expect("a NULL pointer reads");
    assert_eq!(selection, Selection::Platform);
    assert_eq!(selection.posture(), Posture::Platform);
}

#[test]
fn a_pointer_reads_as_the_entry_it_names() {
    let Selection::SelfManaged(own) = pointed().into_selection().expect("a full row reads") else {
        panic!("a pointer is the self-managed posture");
    };
    assert_eq!(own.tenant_model_id.as_str(), POINTER);
    assert_eq!(&*own.model, "claude-opus-5");
    assert_eq!(&*own.secret_ref, "anthropic-prod");
    assert_eq!(own.provider.as_deref(), Some("anthropic"));
    assert_eq!(own.context_cap_tokens, 200_000);
    assert_eq!(
        Selection::SelfManaged(own).posture(),
        Posture::SelfManaged,
        "a pointer means the tenant supplies the key"
    );
}

#[test]
fn a_credential_gone_out_of_band_still_reads_with_no_provider() {
    // The registry page and the tenant view must still render: the
    // missing key is resolution's to refuse, when it opens the vault.
    let row = SelectionRow {
        provider: None,
        ..pointed()
    };
    let Selection::SelfManaged(OwnModel { provider, .. }) = row
        .into_selection()
        .expect("a dangling credential is not a read failure")
    else {
        panic!("a pointer is the self-managed posture");
    };
    assert_eq!(provider, None);
}

#[test]
fn a_pointer_naming_no_readable_entry_is_a_permanent_fault() {
    for row in [
        SelectionRow {
            model_name: None,
            ..pointed()
        },
        SelectionRow {
            secret_ref: None,
            ..pointed()
        },
    ] {
        let refused = row
            .into_selection()
            .expect_err("a pointer at nothing cannot resolve");
        assert!(refused.is_config_permanent());
    }
    pointed_with("not-a-uuid")
        .into_selection()
        .expect_err("a pointer that is not an identifier is refused");
}

#[test]
fn a_negative_catalogue_ceiling_clamps_rather_than_wrapping() {
    let row = SelectionRow {
        context_cap_tokens: -1,
        ..pointed()
    };
    let Selection::SelfManaged(own) = row.into_selection().expect("a full row reads") else {
        panic!("a pointer is the self-managed posture");
    };
    assert_eq!(own.context_cap_tokens, 0);
}

#[test]
fn an_active_default_without_a_model_is_refused_as_corrupt() {
    let row = PlatformDefaultRow {
        provider: "anthropic".to_owned(),
        source_workspace_id: WORKSPACE.to_owned(),
        model_name: None,
        base_url: None,
        context_cap_tokens: None,
    };
    let refused = row
        .into_default()
        .expect_err("an unpriced default is not a default");
    assert!(refused.is_config_permanent());
}

#[test]
fn an_active_default_whose_source_workspace_is_no_identifier_is_refused() {
    let row = PlatformDefaultRow {
        provider: "anthropic".to_owned(),
        source_workspace_id: "not-a-workspace".to_owned(),
        model_name: Some("claude-opus-5".to_owned()),
        base_url: None,
        context_cap_tokens: Some(200_000),
    };
    let refused = row
        .into_default()
        .expect_err("a default whose key lives nowhere nameable cannot resolve");
    assert!(
        !refused.is_config_permanent(),
        "a broken row, not a missing key"
    );
    assert_eq!(refused.code(), afd_core::error_code::INTERNAL_DB_QUERY);
}

fn pointed_with(pointer: &str) -> SelectionRow {
    SelectionRow {
        tenant_model_id: Some(pointer.to_owned()),
        ..pointed()
    }
}
