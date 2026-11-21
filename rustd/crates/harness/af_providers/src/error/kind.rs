//! Every way a provider fails, and every way saved settings are refused.

use std::fmt;

/// A provider's own failure, kept as the cause.
pub(super) type Cause = Box<dyn std::error::Error + Send + Sync>;

/// Every way a provider fails.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    #[error("the decision endpoint could not be parsed")]
    DecisionEndpoint {
        #[source]
        source: url::ParseError,
    },
    #[error("{reason}")]
    DecisionPreflight { reason: &'static str },
    #[error("the decision request could not be built")]
    DecisionBuild {
        #[source]
        source: reqwest::Error,
    },
    #[error("the decision response exceeds its 64 KiB byte bound")]
    DecisionOversize,
    #[error("the decision provider refused the physical call with status {status}")]
    DecisionStatus {
        status: u16,
        retry_after: Option<std::time::Duration>,
    },
    /// The provider refused the turn with a status no retry changes.
    #[error("the model provider refused the turn with status {status}{}", Named(.code.as_deref()))]
    Refused {
        /// The HTTP status.
        status: u16,
        /// The provider's own name for the refusal, such as
        /// `context_length_exceeded`, never its message.
        code: Option<String>,
        /// The original library failure when the provider answered.
        #[source]
        source: Option<Cause>,
    },

    /// The connection was lost before the turn ended.
    #[error("the connection to the model provider was lost mid-turn")]
    Lost {
        /// The transport's reason.
        #[source]
        source: Cause,
    },

    /// The provider ended the turn early with an error of its own.
    #[error("the model provider ended the turn early: {reason}")]
    Ended {
        /// The provider's own name for the error, never its message.
        reason: String,
        /// The provider's typed failure, retained for the diagnostic chain.
        #[source]
        source: Cause,
    },

    /// The provider finished a turn that neither answered nor called a tool,
    /// so the run did nothing a person could read.
    #[error("the model provider {} with no answer", if *.cut { "stopped the turn early" } else { "finished the turn" })]
    Silent {
        /// Whether the provider stopped before it finished the turn, such as
        /// at its output limit, rather than ending it by choice.
        cut: bool,
    },

    /// The provider's reply passed the bytes one turn may carry; the read
    /// ended there.
    #[error("the model provider response exceeds its byte bound of {limit} bytes")]
    Oversize {
        limit: usize,
        #[source]
        source: Cause,
    },

    /// A request or a streamed event was not the JSON its wire defines.
    #[error("the model provider's turn could not be written or read")]
    Unreadable {
        /// The parser's reason.
        #[from]
        source: serde_json::Error,
    },

    /// The wire library could not build the turn's request, or read its
    /// reply as the wire defines it.
    #[error("the model provider's turn could not be built or read")]
    Wire {
        /// The library's reason.
        #[source]
        source: Cause,
    },

    /// The selected model has no saved inference settings.
    #[error("the selected model has no saved inference settings for provider: {provider}")]
    Unhosted {
        /// The provider, as the policy spells it.
        provider: String,
    },

    /// The policy names a `custom:` model endpoint at a private, loopback or
    /// reserved address, which this runner never dials.
    #[error("the policy names a model endpoint at a private or reserved address: {provider}")]
    BlockedEndpoint {
        /// The provider, as the policy spells it.
        provider: String,
    },

    /// The conversation cannot be sent as it stands: a result answers no
    /// call, a call names no tool, or a turn's record will not read. The
    /// words are the wire's.
    #[error(transparent)]
    Conversation { source: guffbolt_ai::Error },

    /// Persisted model settings could not be admitted: the run's view of
    /// a settings refusal, which keeps it as its cause.
    #[error("the selected model's inference settings are invalid")]
    Configuration {
        #[source]
        source: super::Error,
    },

    /// The route the settings name cannot carry this turn: its input leaves
    /// the window no room, or the wire cannot encode it.
    #[error("the selected model's inference settings are invalid")]
    Route {
        #[source]
        source: guffbolt_ai::Error,
    },

    /// The HTTP client could not be built.
    #[error("the model providers' HTTP client could not be built")]
    Client {
        /// The client's reason. Lifted by name, never by `?`: a send that
        /// fails is a lost connection, not a client that could not be built.
        #[source]
        source: reqwest::Error,
    },

    /// Saved settings pass their declared bounds.
    #[error("inference settings exceed their declared bounds")]
    Bounds {
        #[source]
        source: garde::Report,
    },

    /// A saved endpoint is not a URL.
    #[error("inference endpoint is not a valid URL")]
    Url {
        #[source]
        source: url::ParseError,
    },

    /// A saved endpoint is not a credential-free public HTTPS base.
    #[error("inference endpoint must be HTTPS without userinfo, query or fragment")]
    Endpoint,

    /// Active decision settings lack a task primitive they need.
    #[error("active decision settings require Noul and Choice capabilities")]
    DecisionCapabilities,

    /// The route saved settings name is unusable: its words are the route's.
    #[error(transparent)]
    Unroutable { source: guffbolt_ai::Error },
}

/// A refusal's code in parentheses after its status, when the provider named
/// one: `status 400 (context_length_exceeded)`.
struct Named<'a>(Option<&'a str>);

impl fmt::Display for Named<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.map_or(Ok(()), |code| write!(f, " ({code})"))
    }
}
