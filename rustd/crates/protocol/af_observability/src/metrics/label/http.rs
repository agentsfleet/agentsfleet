//! What the HTTP surface and the exporter label their measurements with.

use crate::metrics::label::closed_set;
use crate::semconv;

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
        /// The configured provider is outside the compiled public naming registry.
        UnmappedProvider => "unmapped_provider",
        /// Attributing this pair would push the window past its series budget.
        BudgetExhausted => "budget_exhausted",
        /// The value is longer than the payload admits, and truncating it
        /// would export a DIFFERENT model under a plausible-looking name.
        ValueTooLong => "value_too_long",
    }
}

closed_set! {
    /// Bounded methods for request metrics, including extension methods.
    RequestMethod {
        /// HTTP GET.
        Get => "GET",
        /// HTTP POST.
        Post => "POST",
        /// HTTP PUT.
        Put => "PUT",
        /// HTTP PATCH.
        Patch => "PATCH",
        /// HTTP DELETE.
        Delete => "DELETE",
        /// HTTP OPTIONS.
        Options => "OPTIONS",
        /// HTTP HEAD.
        Head => "HEAD",
        /// An extension method never becomes its own series.
        Other => "_other"
    }
}
impl RequestMethod {
    /// Maps an HTTP method into the fixed metric vocabulary.
    #[must_use]
    pub fn of(method: &str) -> Self {
        match method {
            "GET" => Self::Get,
            "POST" => Self::Post,
            "PUT" => Self::Put,
            "PATCH" => Self::Patch,
            "DELETE" => Self::Delete,
            "OPTIONS" => Self::Options,
            "HEAD" => Self::Head,
            _ => Self::Other,
        }
    }
}
closed_set! {
    /// Responses and cancellation are distinct terminal observations.
    ResponseClass {
        /// Informational response.
        Informational => "1xx",
        /// Successful response.
        Success => "2xx",
        /// Redirect.
        Redirect => "3xx",
        /// Client refusal.
        ClientError => "4xx",
        /// Server failure.
        ServerError => "5xx",
        /// Unrecognized response class.
        Other => "_other",
        /// The request future was dropped before headers.
        Cancelled => "cancelled"
    }
}
impl ResponseClass {
    /// Groups a status without allowing arbitrary status labels.
    #[must_use]
    pub const fn of(status: u16) -> Self {
        match status / 100 {
            1 => Self::Informational,
            2 => Self::Success,
            3 => Self::Redirect,
            4 => Self::ClientError,
            5 => Self::ServerError,
            _ => Self::Other,
        }
    }
}
