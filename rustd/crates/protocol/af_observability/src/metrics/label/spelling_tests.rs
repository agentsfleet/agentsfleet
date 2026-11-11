//! No closed set spells two of its members alike.

/// Every closed set this build declares, with the spellings it can write.
///
/// Named one by one on purpose: a set added to the module and not to this list
/// is a set nothing grades, and deriving the list would only compare the
/// module with itself.
fn every_closed_set() -> Vec<(&'static str, Vec<&'static str>)> {
    macro_rules! set {
        ($path:path) => {{
            use $path as Set;
            (
                stringify!($path),
                Set::ALL.iter().map(|member| member.as_str()).collect(),
            )
        }};
    }
    vec![
        set!(crate::metrics::label::bus::GapCause),
        set!(crate::metrics::label::outbound::ClaimVia),
        set!(crate::metrics::label::fleet::StreamPacket),
        set!(crate::metrics::label::fleet::StreamTransport),
        set!(crate::metrics::label::fleet::StreamOutcome),
        set!(crate::metrics::label::fleet::StreamStage),
        set!(crate::metrics::label::fleet::StreamFallback),
        set!(crate::metrics::label::fleet::WakeQueueOutcome),
        set!(crate::metrics::label::task::TaskOperation),
        set!(crate::metrics::label::task::TaskOutcome),
        set!(crate::metrics::label::database::PoolRole),
        set!(crate::metrics::label::database::PoolState),
        set!(crate::metrics::label::database::AcquireOutcome),
        set!(crate::metrics::label::models::CallOutcome),
        set!(crate::metrics::label::scheduler::FireOutcome),
        set!(crate::metrics::label::scheduler::SyncOutcome),
        set!(crate::metrics::label::scheduler::ScheduleState),
        set!(crate::metrics::label::http::RequestMethod),
        set!(crate::metrics::label::http::ResponseClass),
        set!(crate::metrics::label::cost::TokenType),
        set!(crate::metrics::label::cost::ChargeClass),
        set!(crate::metrics::label::cost::ErrorType),
        set!(crate::metrics::label::fleet::SignupFailure),
        set!(crate::metrics::label::fleet::HistoryCut),
        set!(crate::metrics::label::fleet::HeldClaim),
        set!(crate::metrics::label::fleet::LeaseCache),
        set!(crate::metrics::label::fleet::CacheOutcome),
        set!(crate::metrics::label::fleet::AdmissionOutcome),
        set!(crate::metrics::label::http::Signal),
        set!(crate::metrics::label::http::DiscardReason),
        set!(crate::metrics::label::http::OmittedAttribute),
        set!(crate::metrics::label::http::OmissionReason),
        set!(crate::metrics::label::library::Surface),
        set!(crate::metrics::label::library::Stage),
        set!(crate::metrics::label::library::ReadOutcome),
    ]
}

/// No closed set spells two of its members alike.
///
/// The ceiling test above multiplies `ALL.len()`, so a copy-pasted arm that
/// gives two variants one string understates nothing and still collapses two
/// series into one — two different outcomes drawn as a single line, with the
/// budget still reporting room for both.
#[test]
fn no_closed_set_spells_two_members_alike() {
    for (set, spellings) in every_closed_set() {
        let distinct: std::collections::BTreeSet<&str> = spellings.iter().copied().collect();
        assert_eq!(
            distinct.len(),
            spellings.len(),
            "`{set}` spells two of its {} members alike, which is one series \
             where the ceiling counts two",
            spellings.len()
        );
    }
}
