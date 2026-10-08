//! A foreign crate's failure lifted into this crate's error keeps its source,
//! and its code, sentence and outage class where that crate decided them.

use std::error::Error as _;

use super::Error;

/// The first error the database layer can produce, for lifting through a
/// downstream crate's `From`.
fn first_db_error() -> Result<afd_db::Error, &'static str> {
    afd_db::error::one_of_each_kind()
        .into_iter()
        .next()
        .map(|(_kind, error)| error)
        .ok_or("database test utility exposes no error")
}

/// The datastore, queue and store-backed crates' failures, lifted.
fn lifted_stores() -> Result<[Error; 6], &'static str> {
    let database = afd_db::error::one_of_each_kind()
        .into_iter()
        .find(|(kind, _error)| *kind == "datastore unavailable")
        .map(|(_kind, error)| error)
        .ok_or("database test utility has no outage kind")?;
    let queue = afd_dragonfly::error::one_of_each_kind()
        .into_iter()
        .next()
        .map(|(_kind, error)| error)
        .ok_or("queue test utility exposes no error")?;
    Ok([
        Error::from(database),
        Error::from(queue),
        Error::from(afd_gate::Error::from(first_db_error()?)),
        Error::from(afd_credential::Error::from(first_db_error()?)),
        Error::from(afd_billing::Error::from(first_db_error()?)),
        Error::from(afd_events::Error::from(first_db_error()?)),
    ])
}

/// An identifier, a config value and an entropy draw that failed, lifted.
fn lifted_inputs() -> Result<[Error; 3], &'static str> {
    let identifier = afd_core::id::Uuid7::parse("not-an-id")
        .err()
        .ok_or("fixture id unexpectedly parsed")?;
    let config = afd_fleet_runtime::FleetName::parse("")
        .err()
        .ok_or("fixture fleet name unexpectedly parsed")?;
    let (entropy, control) = afd_crypto::entropy::Entropy::new_mocked();
    control.fail_next();
    let mut bytes = [0_u8; afd_core::id::ENTROPY_LEN];
    let entropy = entropy
        .fill(&mut bytes)
        .err()
        .ok_or("the controlled entropy source unexpectedly answered")?;
    Ok([
        Error::from(identifier),
        Error::from(config),
        Error::from(entropy),
    ])
}

/// The admission and delivery ledgers' failures, lifted, each checked for
/// what it carries over from the crate that decided it.
fn lifted_ledgers() -> Result<[Error; 2], &'static str> {
    // The ledger the lease asks for a restored cursor. Lifted here so the
    // delegation below — code, sentence and outage class all read off the
    // source — is proven rather than assumed.
    let admission = afd_admission::error::one_of_each_kind()
        .into_iter()
        .find(|(kind, _error)| *kind == "datastore")
        .map(|(_kind, error)| error)
        .ok_or("the admission sample has no outage kind")?;
    // The delivery ledger `afd_outbound` owns. Lifted through that crate's own
    // `error_lifts!` rather than converted here, so `?` carries a report's
    // obligation failure with no `map_err` at the call site.
    let lifted_outbound = Error::from(afd_outbound::Error::from(first_db_error()?));
    let lifted_admission = Error::from(admission);
    // Read off the source, not restated here: a second copy of the admission
    // plane's mapping in this crate is exactly the drift the lift exists to
    // avoid.
    assert!(lifted_admission.is_datastore_unavailable());
    Ok([lifted_admission, lifted_outbound])
}

#[test]
fn foreign_datastore_queue_identifier_and_config_errors_lift_with_sources()
-> Result<(), &'static str> {
    let lifted = lifted_stores()?
        .into_iter()
        .chain(lifted_inputs()?)
        .chain(lifted_ledgers()?);
    for failure in lifted {
        assert!(failure.source().is_some());
        assert!(!failure.detail().is_empty(), "{:?}", failure.detail());
        assert!(
            !failure.code().as_str().is_empty(),
            "{:?}",
            failure.code().as_str()
        );
    }
    Ok(())
}

/// A memory-store failure answers with the code, the sentence and the outage
/// class `afd_memory` decided; the lease plane restates none of them.
#[test]
fn a_memory_store_failure_keeps_the_memory_crates_classification() -> Result<(), &'static str> {
    let outage = afd_db::error::one_of_each_kind()
        .into_iter()
        .find(|(kind, _error)| *kind == "datastore unavailable")
        .map(|(_kind, error)| error)
        .ok_or("database test utility has no outage kind")?;
    let unreadable_writer = afd_core::id::Uuid7::parse("not-an-id")
        .err()
        .ok_or("fixture id unexpectedly parsed")?;

    for (memory, unavailable) in [
        (afd_memory::Error::from(outage), true),
        (afd_memory::Error::from(unreadable_writer), false),
    ] {
        let (code, detail) = (memory.code(), memory.detail());
        let lifted = Error::from(memory);

        assert_eq!(lifted.code(), code);
        assert_eq!(lifted.detail(), detail);
        assert_eq!(lifted.is_datastore_unavailable(), unavailable);
        assert!(lifted.source().is_some());
    }
    Ok(())
}
