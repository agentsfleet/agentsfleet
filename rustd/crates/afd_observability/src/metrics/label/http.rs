//! What the HTTP surface and the exporter label their measurements with.

use crate::metrics::label::closed_set;
use crate::semconv;

closed_set! {
    /// Why a request that would have opened a span did not.
    ///
    /// `route_trace.zig`'s own set, kept whole: the budget exists so a storm of
    /// idle heartbeats cannot evict the server-error spans, and an operator
    /// reading suppression needs to know WHICH budget shed — a total would say
    /// only that something did.
    TraceSuppression {
        /// A route excluded from tracing outright.
        NoisyRoute => "noisy_route",
        /// The budget for matched runner 4xx responses was spent.
        RunnerRejectionBudget => "runner_rejection_budget",
        /// The budget for responses at or above 500 was spent.
        ServerErrorBudget => "server_error_budget",
        /// The budget for sampled successes was spent.
        SampledSuccessBudget => "sampled_success_budget",
        /// Head sampling declined this one, with budget still available.
        SampleMiss => "sample_miss",
    }
}

closed_set! {
    /// Which of the three OTLP signals a measurement is about.
    Signal {
        /// Log records.
        Logs => "logs",
        /// Spans.
        Traces => "traces",
        /// Metric data points.
        Metrics => "metrics",
    }
}

closed_set! {
    /// Why the exporter lost something before it reached a collector.
    ///
    /// Loss counted at the SOURCE, which is the point: the exporter's own
    /// dropped-batch counters ride the same push they describe, so a dead pipe
    /// cannot report on itself. These say what was lost locally and why.
    DiscardReason {
        /// The queue was full when the entry arrived.
        RingFull => "ring_full",
        /// The aggregation's series cap folded it away.
        AggregateCap => "aggregate_cap",
        /// The batch could not be encoded.
        SerializeFailed => "serialize_failed",
        /// The collector accepted the batch and rejected part of it.
        PartialRejected => "partial_rejected",
        /// The collector refused the batch outright.
        ExportRejected => "export_rejected",
        /// The attempt neither succeeded nor failed definitely — a timeout, or
        /// a transport that went away mid-send.
        ExportUncertain => "export_uncertain",
    }
}

closed_set! {
    /// Which attribute this process declined to put on a data point.
    ///
    /// Both are optional in the contract, so omitting one leaves a valid point
    /// — which is what makes omission the honest answer rather than a defect.
    OmittedAttribute {
        /// `gen_ai.provider.name`.
        ProviderName => semconv::ATTR_PROVIDER_NAME,
        /// `gen_ai.request.model`.
        RequestModel => semconv::ATTR_REQUEST_MODEL,
    }
}

closed_set! {
    /// Why an attribute was omitted rather than written.
    OmissionReason {
        /// The configured provider maps to no well-known name.
        UnmappedProvider => "unmapped_provider",
        /// Attributing this pair would push the window past its series budget.
        BudgetExhausted => "budget_exhausted",
        /// The value is longer than the payload admits, and truncating it
        /// would export a DIFFERENT model under a plausible-looking name.
        ValueTooLong => "value_too_long",
    }
}
