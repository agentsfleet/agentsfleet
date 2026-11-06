//! Bounded dimensions for native decision evaluation.

/// Which reviewed decision purpose a measurement serves.
pub const LABEL_PURPOSE: &str = "purpose";

afd_observability::closed_set! {
    /// Native decision protocol, independently of coding-provider vendor names.
    DecisionProvider {
        /// `TypeSafe` System One.
        Typesafe => "typesafe",
        /// `OpenAI` Decisions.
        Openai => "openai",
        /// Perplexity Decisions.
        Perplexity => "perplexity",
        /// Cloudflare Clef.
        Cloudflare => "cloudflare",
    }
}

afd_observability::closed_set! {
    /// The native runtime's reason for evaluating evidence.
    DecisionPurpose {
        /// Interpret the newest input.
        Intent => "intent",
        /// Assess progress against the accepted objective.
        Goal => "goal",
    }
}

afd_observability::closed_set! {
    /// What became of one bounded evaluation, including a denied preflight.
    DecisionOutcome {
        /// Complete typed answers arrived.
        Completed => "completed",
        /// Provider refused a required question.
        Refused => "refused",
        /// Reported model was not accepted by the frozen profile.
        InvalidModel => "invalid_model",
        /// Required accounting was missing or inconsistent.
        InvalidUsage => "invalid_usage",
        /// Required answers were incomplete or inconsistent.
        InvalidAnswers => "invalid_answers",
        /// Input failed before transport began.
        NotSent => "not_sent",
        /// The provider requested slower traffic.
        RateLimited => "rate_limited",
        /// A different provider status refused the call.
        ProviderFailure => "provider_failure",
        /// The physical send or complete response exceeded its time bound.
        Timeout => "timeout",
        /// The transport ended without a confirmed result.
        Unknown => "unknown",
        /// Response bytes exceeded their bound.
        Oversize => "oversize",
        /// Response bytes were not JSON.
        Malformed => "malformed",
        /// The caller stopped a dispatched evaluation.
        Cancelled => "cancelled",
    }
}
