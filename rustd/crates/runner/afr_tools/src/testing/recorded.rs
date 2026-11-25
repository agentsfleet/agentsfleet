//! The recording fakes: a run's verbs answering from a script, and what each
//! call asked, compared whole.

use afr_memory::Loaded;

use crate::run::Run;

/// One call a tool made through the run's verbs, owned so a suite can read
/// it after the call returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Asked {
    /// A schedules call, field by field.
    Schedules(OwnedCall),
    /// A message, as the text that left the runner.
    Message(String),
    /// A fleets read, field by field.
    Fleets(OwnedFleetsCall),
    /// A steer, as the fleet asked and the text that left the runner.
    Steer {
        /// The fleet the work was put on.
        fleet: String,
        /// The text, as masked.
        text: String,
    },
    /// A triggers call, its trigger owned.
    Triggers(OwnedTriggerCall),
    /// An access request, as it left the runner.
    Access(af_wire::access_verb::AccessRequest<'static>),
    /// A binding, as it left the runner.
    Bindings(af_wire::durable_task::DurableTaskBindingsInput),
}

/// A [`crate::verbs::TriggerCall`] with every field owned, compared whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OwnedTriggerCall {
    Add {
        call_number: u64,
        trigger: crate::verbs::NewTrigger<'static>,
    },
    List,
    Remove(String),
}

/// `call` with every text it borrows owned.
fn owned_trigger(call: crate::verbs::TriggerCall<'_>) -> OwnedTriggerCall {
    use crate::verbs::{NewTrigger, TriggerCall};
    use std::borrow::Cow;
    let text = |text: Option<Cow<'_, str>>| text.map(|text| Cow::Owned(text.into_owned()));
    let list = |list: Option<Vec<Cow<'_, str>>>| {
        list.map(|texts| {
            texts
                .into_iter()
                .map(|t| Cow::Owned(t.into_owned()))
                .collect()
        })
    };
    match call {
        TriggerCall::Add {
            call_number,
            trigger,
        } => OwnedTriggerCall::Add {
            call_number,
            trigger: NewTrigger {
                kind: trigger.kind,
                source: text(trigger.source),
                events: list(trigger.events),
                repositories: list(trigger.repositories),
                verifier: trigger
                    .verifier
                    .map(|verifier| af_wire::trigger_verb::TriggerVerifier {
                        scheme: Cow::Owned(verifier.scheme.into_owned()),
                        header: text(verifier.header),
                    }),
                secret_name: text(trigger.secret_name),
                channel: text(trigger.channel),
            },
        },
        TriggerCall::List => OwnedTriggerCall::List,
        TriggerCall::Remove { trigger } => OwnedTriggerCall::Remove(trigger.as_str().to_owned()),
    }
}

/// A [`crate::verbs::FleetsCall`] with every field owned, compared whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OwnedFleetsCall {
    List {
        limit: Option<u32>,
        starting_after: Option<String>,
    },
    Events {
        fleet: String,
        limit: Option<u32>,
        starting_after: Option<String>,
    },
}

impl From<crate::verbs::FleetsCall<'_>> for OwnedFleetsCall {
    fn from(call: crate::verbs::FleetsCall<'_>) -> Self {
        match call {
            crate::verbs::FleetsCall::List {
                limit,
                starting_after,
            } => Self::List {
                limit,
                starting_after: starting_after.map(str::to_owned),
            },
            crate::verbs::FleetsCall::Events {
                fleet,
                limit,
                starting_after,
            } => Self::Events {
                fleet: fleet.as_str().to_owned(),
                limit,
                starting_after: starting_after.map(str::to_owned),
            },
        }
    }
}

/// A [`crate::verbs::ScheduleCall`] with every field owned, compared whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OwnedCall {
    Create {
        cron: String,
        timezone: Option<String>,
        message: String,
        once: bool,
    },
    List,
    Update {
        schedule: String,
        cron: Option<String>,
        timezone: Option<String>,
        message: Option<String>,
        paused: Option<bool>,
    },
    Delete(String),
    Run(String),
    Runs {
        schedule: String,
        limit: Option<u32>,
        starting_after: Option<String>,
    },
}

impl From<crate::verbs::ScheduleCall<'_>> for OwnedCall {
    fn from(call: crate::verbs::ScheduleCall<'_>) -> Self {
        use crate::verbs::ScheduleCall;
        let owned = |text: Option<&str>| text.map(str::to_owned);
        match call {
            ScheduleCall::Create {
                cron,
                timezone,
                message,
                once,
                ..
            } => Self::Create {
                cron: cron.to_owned(),
                timezone: owned(timezone),
                message: message.to_owned(),
                once,
            },
            ScheduleCall::List => Self::List,
            ScheduleCall::Update {
                schedule,
                cron,
                timezone,
                message,
                paused,
                ..
            } => Self::Update {
                schedule: schedule.as_str().to_owned(),
                cron: owned(cron),
                timezone: owned(timezone),
                message: owned(message),
                paused,
            },
            ScheduleCall::Delete { schedule } => Self::Delete(schedule.as_str().to_owned()),
            ScheduleCall::Run { schedule, .. } => Self::Run(schedule.as_str().to_owned()),
            ScheduleCall::Runs {
                schedule,
                limit,
                starting_after,
            } => Self::Runs {
                schedule: schedule.as_str().to_owned(),
                limit,
                starting_after: owned(starting_after),
            },
        }
    }
}

/// What a recorded binding answers with.
type Bound =
    Result<af_wire::durable_task::DurableTaskBindingsResponse<'static>, crate::verbs::Unanswered>;

/// Run verbs answering every call with `answer`, recording what was asked.
#[derive(Debug)]
pub(crate) struct RecordingVerbs {
    answer: Result<String, crate::verbs::Unanswered>,
    delivered: Result<bool, crate::verbs::Unanswered>,
    steered: Result<af_wire::steer_verb::FleetSteerAccepted<'static>, crate::verbs::Unanswered>,
    access: Result<crate::verbs::AccessAnswer, crate::verbs::Unanswered>,
    bound: Bound,
    asked: std::sync::Mutex<Vec<Asked>>,
}

/// The event a recorded steer answers with, unless a suite says otherwise.
pub(crate) const STEERED_EVENT: &str = "0190d5c2-7e1a-7000-8000-00000000e0f1";

impl RecordingVerbs {
    /// Verbs answering schedules calls with `answer` and messages with
    /// `delivered`.
    pub(crate) fn answering(
        answer: Result<String, crate::verbs::Unanswered>,
        delivered: Result<bool, crate::verbs::Unanswered>,
    ) -> Self {
        Self {
            answer,
            delivered,
            steered: Ok(af_wire::steer_verb::FleetSteerAccepted {
                turn_id: std::borrow::Cow::Borrowed(STEERED_EVENT),
                replayed: false,
            }),
            access: Ok(crate::verbs::AccessAnswer::Asked),
            bound: Err(crate::verbs::Unanswered::Unreachable),
            asked: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// The same verbs, answering an access request with `access`.
    pub(crate) fn granting(
        mut self,
        access: Result<crate::verbs::AccessAnswer, crate::verbs::Unanswered>,
    ) -> Self {
        self.access = access;
        self
    }

    /// The same verbs, answering a binding with `bound`.
    pub(crate) fn binding(mut self, bound: Bound) -> Self {
        self.bound = bound;
        self
    }

    /// The same verbs, answering a steer with `steered`.
    pub(crate) fn steering(
        mut self,
        steered: Result<af_wire::steer_verb::FleetSteerAccepted<'static>, crate::verbs::Unanswered>,
    ) -> Self {
        self.steered = steered;
        self
    }

    /// Everything asked so far, in order.
    pub(crate) fn asked(&self) -> Vec<Asked> {
        self.asked.lock().map_or_default(|asked| asked.clone())
    }

    fn record(&self, asked: Asked) {
        self.asked
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(asked);
    }
}

#[async_trait::async_trait]
impl crate::verbs::RunVerbs for RecordingVerbs {
    async fn schedules(
        &self,
        call: crate::verbs::ScheduleCall<'_>,
    ) -> Result<String, crate::verbs::Unanswered> {
        self.record(Asked::Schedules(OwnedCall::from(call)));
        self.answer.clone()
    }

    async fn message(
        &self,
        _call_number: u64,
        text: &str,
    ) -> Result<bool, crate::verbs::Unanswered> {
        self.record(Asked::Message(text.to_owned()));
        self.delivered
    }

    async fn fleets(
        &self,
        call: crate::verbs::FleetsCall<'_>,
    ) -> Result<String, crate::verbs::Unanswered> {
        self.record(Asked::Fleets(OwnedFleetsCall::from(call)));
        self.answer.clone()
    }

    async fn fleet_steer(
        &self,
        fleet: &af_core::id::Uuid7,
        text: &str,
    ) -> Result<af_wire::steer_verb::FleetSteerAccepted<'static>, crate::verbs::Unanswered> {
        self.record(Asked::Steer {
            fleet: fleet.as_str().to_owned(),
            text: text.to_owned(),
        });
        self.steered.clone()
    }

    async fn triggers(
        &self,
        call: crate::verbs::TriggerCall<'_>,
    ) -> Result<String, crate::verbs::Unanswered> {
        self.record(Asked::Triggers(owned_trigger(call)));
        self.answer.clone()
    }

    async fn access(
        &self,
        request: af_wire::access_verb::AccessRequest<'_>,
    ) -> Result<crate::verbs::AccessAnswer, crate::verbs::Unanswered> {
        self.record(Asked::Access(af_wire::access_verb::AccessRequest {
            fencing_token: request.fencing_token,
            name: std::borrow::Cow::Owned(request.name.into_owned()),
            scope: request.scope,
        }));
        self.access
    }

    async fn durable_task_bindings(
        &self,
        input: af_wire::durable_task::DurableTaskBindingsInput,
    ) -> Bound {
        self.record(Asked::Bindings(input));
        self.bound.clone()
    }
}

/// A run whose verbs are `verbs`, with empty memory and closed egress.
pub(crate) fn run_with(verbs: &dyn crate::verbs::RunVerbs) -> Run<'_> {
    Run::new(Loaded::default(), af_egress::testing::closed(), verbs)
}
