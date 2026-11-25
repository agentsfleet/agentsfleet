//! A fleet's trigger rows, written through the store a run's `trigger_add`
//! writes through, so a delivery resolves against the rows production holds.

use af_core::id::Uuid7;
use af_wire::trigger_verb::TriggerKind;
use afd_db::Db;
use afd_setup::trigger::NewTrigger;

/// One trigger to seed on a fleet; every field `None` the store leaves null.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Seeded<'a> {
    /// What wakes the fleet.
    pub(crate) kind: TriggerKind,
    /// A webhook's source slug, `github` for an App event, `slack` for a mention.
    pub(crate) source: &'a str,
    /// The event names that wake the fleet.
    pub(crate) events: Option<&'a [String]>,
    /// The repositories whose deliveries wake the fleet.
    pub(crate) repositories: Option<&'a [String]>,
    /// A declared recipe and its header, for a source outside the registry.
    pub(crate) verifier: Option<(&'a str, Option<&'a str>)>,
    /// The vault key of the signing secret.
    pub(crate) secret_name: Option<&'a str>,
    /// A mention's channel identifier.
    pub(crate) channel: Option<&'a str>,
}

impl<'a> Seeded<'a> {
    /// A trigger of `kind` from `source`, with nothing else set.
    pub(crate) const fn new(kind: TriggerKind, source: &'a str) -> Self {
        Self {
            kind,
            source,
            events: None,
            repositories: None,
            verifier: None,
            secret_name: None,
            channel: None,
        }
    }
}

/// Writes `seeded` on `fleet` in `workspace`, answering the row's id.
///
/// # Panics
/// When the store refuses the row or the fleet already holds its kind and
/// source: a fixture that seeded a duplicate has a broken precondition.
#[expect(
    clippy::expect_used,
    reason = "a fixture whose precondition fails should stop the case"
)]
pub(crate) async fn seed(
    database: &Db,
    fleet: &Uuid7,
    workspace: &Uuid7,
    seeded: Seeded<'_>,
) -> Uuid7 {
    let id = afd_db::test_util::mint_id();
    let mut connection = database.acquire().await.expect("a trigger connection");
    afd_setup::trigger::insert(
        &mut connection,
        NewTrigger {
            id: &id,
            fleet,
            workspace,
            trigger_type: seeded.kind,
            source: seeded.source,
            events: seeded.events,
            repositories: seeded.repositories,
            verifier_scheme: seeded.verifier.map(|(scheme, _)| scheme),
            verifier_header: seeded.verifier.and_then(|(_, header)| header),
            secret_name: seeded.secret_name,
            channel: seeded.channel,
        },
    )
    .await
    .expect("the trigger row seeds")
    .expect("the fleet held no trigger of this kind and source");
    id
}
