//! What a caller is TOLD, as distinct from what went wrong.
//!
//! Every sentence here is client-visible, so each is a wire fact: what a caller
//! reads is behaviour (RULE UFS). They live apart from [`super::ErrorKind`]
//! because the two answer different questions — that one is what happened, this
//! is what we say — and because a sentence changing is a wire change while a
//! kind changing is not.

/// The enrolment refusal when `host_id` is absent or too long.
///
/// Client-visible, so changing it is a wire change: what a caller reads is
/// behaviour (RULE UFS).
pub const DETAIL_HOST_ID_BOUNDS: &str = "host_id must be 1-256 chars";

/// The enrolment refusal for a malformed registry allowlist entry.
pub const DETAIL_REGISTRY_ALLOWLIST: &str = "registry_allowlist entries must be host[:port] names";

/// The refusal when the token authenticated and the runner row is gone.
pub const DETAIL_RUNNER_NOT_FOUND: &str = "runner not found";

/// The refusal when a terminal runner cannot collect a self-test ask.
pub const DETAIL_SELFTEST_REFUSED: &str = "revoked runners cannot be asked to self-test";

/// The database-outage detail every crate shares through `afd_core::error`.
pub use afd_core::error::DETAIL_DATABASE_UNAVAILABLE;

/// The database-fault detail every crate shares through `afd_core::error`.
pub use afd_core::error::DETAIL_DATABASE_ERROR;

/// An event on the stream this daemon cannot execute.
///
/// The caller is a runner asking for work, so what it reads says nothing about
/// its own request — the missing field goes to the log, where an operator can
/// correlate it with the producer that wrote the entry.
pub const DETAIL_EVENT_MALFORMED: &str = "leased event malformed";

/// A queue outage, shaped like its database counterpart above.
///
/// A detail is not optional here, and answering "Database unavailable" for a
/// Dragonfly outage would send an operator to the wrong datastore. The CODE is
/// `UZ-INTERNAL-003`, shared with the rest of the internal-failure family in
/// `classify.rs` — no registry entry of its own, so the ERROR REGISTRY gate
/// does not fire.
pub const DETAIL_QUEUE_UNAVAILABLE: &str = "Queue unavailable";

/// The detail when enrolment could not mint the identifier a client waits on.
///
/// Reached only from the enrolment path: it is the one verb that mints an
/// identifier a client is waiting on, and every other mint in this crate is
/// best-effort and never surfaces.
pub const DETAIL_REGISTRATION_FAILED: &str = "runner registration failed";

/// A lease whose tenant's provider could not be resolved.
///
/// A detail is not optional here, and this one deliberately says nothing about
/// WHICH part of the configuration is broken — the caller is a runner asking
/// for work, and the tenant's vault layout is not its business. The operator
/// gets the field name in the log.
pub const DETAIL_PROVIDER_UNRESOLVED: &str = "provider unresolved";

/// A fleet naming a credential the vault does not hold.
///
/// The registry sentence for `UZ-AGT-003`, which is what an operator reads on
/// every other surface that raises this code. The lease path
/// itself never writes it — it ends the event and answers no-work — but a
/// detail is not optional, and answering a DIFFERENT sentence for the same code
/// on one surface is how a runbook stops matching what the product says.
pub const DETAIL_CREDENTIAL_MISSING: &str =
    "A required credential is not in the vault. Add it with: `agentsfleet secret create <NAME>`";

/// A stored credential body that is not an addressable JSON object.
///
/// The `UZ-VAULT-001` registry TITLE, byte-identical, rather than a sentence
/// invented here: the tenant plane's create/replace routes answer this code for
/// the same fact, and the lease path meeting it in stored data is that
/// validation having been bypassed — `storeJsonPlaintext` skips the shape gate
/// by design, so a malformed body can reach the vault. One fact, one sentence.
pub const DETAIL_VAULT_DATA_INVALID: &str = "Secret data must be a non-empty JSON object";

/// A fleet's stored configuration could not be read.
///
/// The failure can reach a caller, so it needs a sentence — and this one names
/// the stored document rather than the request, because the runner did nothing
/// wrong and the fix is in the fleet.
pub const DETAIL_CONFIG_UNREADABLE: &str = "fleet configuration unreadable";

/// A gate reference that could not be written.
///
/// Unreachable for the shape the gate stores — a record of a string and an
/// integer — and present because the alternative is swallowing a failure that
/// would leave a parked event unable to find its own gate. Never rendered: the
/// lease path answers no-work rather than surfacing this.
pub const DETAIL_GATE_REFERENCE_UNWRITABLE: &str = "approval gate reference unwritable";

/// An approved reach that could not be recorded.
///
/// The sibling of [`DETAIL_GATE_REFERENCE_UNWRITABLE`], and unreachable for the
/// same kind of reason: the recorded binding is a list of strings, an enum and
/// an optional string, none of which has a serializer failure to reach. Present
/// because the alternative is a gate row whose `stated_binding` is `NULL` — and
/// the write mint refuses those, so swallowing it would turn an impossible
/// failure into an approval nobody can spend. Never rendered.
pub const DETAIL_GATE_BINDING_UNWRITABLE: &str = "approval gate binding unwritable";

/// The report refusal when the presenting holder has been superseded.
///
/// It names the outcome — the current holder's result wins — because that is
/// the fact the runner acts on: it stops retrying and discards its own result,
/// rather than backing off and re-reporting into a lease it no longer holds.
pub const DETAIL_STALE_FENCE: &str = "Lease superseded by a newer holder; report rejected";

/// The renewal refusal when no lease with that id is the caller's.
///
/// Deliberately says nothing about WHICH of the two happened — no such lease,
/// or somebody else's lease. The load is scoped by runner, so this sentence is
/// all either case can honestly claim to know, and a sharper one would make the
/// endpoint an oracle for live lease ids.
pub const DETAIL_LEASE_NOT_FOUND: &str = "No lease matches this lease_id for the runner";

/// The renewal refusal when the lease moved on before this renewal.
///
/// One sentence for two observations — the status check finding the lease no
/// longer active, and the atomic extend finding it reassigned. Both are the
/// same fact observed a moment apart, and the runner's remedy is identical
/// either way: terminate the child. Two sentences would suggest a distinction
/// it could act on and cannot.
pub const DETAIL_LEASE_LOST: &str = "Lease was reassigned before this renewal; terminate the child";

/// The renewal refusal at the hard runtime ceiling.
pub const DETAIL_LEASE_MAX_RUNTIME: &str = "Lease reached the hard max runtime; not renewed";

/// The renewal refusal when the TENANT's credit pool is spent.
///
/// Distinct from [`DETAIL_BUDGET_EXHAUSTED`] beside it, and the two sentences
/// are the only thing that tells an operator which pool to look at: this one is
/// topped up, that one is edited in `TRIGGER.md`.
pub const DETAIL_RENEWAL_NO_CREDITS: &str =
    "Tenant balance can no longer fund this run; not renewed";

/// The renewal refusal when the FLEET's own ceiling is reached.
pub const DETAIL_BUDGET_EXHAUSTED: &str = "Fleet budget exhausted for this window; not renewed";

/// The bundle answer when nothing is stored under a content hash.
///
/// Reads as a statement of fact rather than as a fault, because it is one: a
/// bundle with no support files stores no snapshot, so a runner meeting this
/// proceeds with none.
pub const DETAIL_BUNDLE_NOT_FOUND: &str = "no snapshot stored for this content hash";

/// The bundle answer when snapshot storage is not configured at all.
pub const DETAIL_BUNDLE_STORAGE_UNAVAILABLE: &str = "Fleet Bundle snapshot storage is unavailable";

/// The bundle answer when the store was reached and would not serve.
///
/// Distinct from [`DETAIL_BUNDLE_STORAGE_UNAVAILABLE`] beside it, under one
/// code: an operator reading the first goes and
/// sets four knobs, and reading the second goes and looks at the bucket. The
/// runner cannot act on the difference and is not asked to — both are 503s it
/// re-polls past.
pub const DETAIL_BUNDLE_FETCH_FAILED: &str = "Fleet Bundle snapshot fetch failed";

/// The mint refusal when the workspace has no connected integration under
/// the handle.
///
/// Answers BOTH a workspace that connected nothing under this name and a handle
/// naming a connector this registry does not carry. One sentence for both, and
/// deliberately: a runner acts identically on either, and telling them apart
/// would make the mint an oracle for which connectors a deployment ships.
pub const DETAIL_INTEGRATION_NOT_CONNECTED: &str = "Integration not connected for this workspace";

/// The mint refusal when this deployment holds no broker credential.
///
/// An OPERATOR's fault, and the sentence says so: no tenant action reaches it,
/// because what is missing is this deployment's own platform credential.
pub const DETAIL_MINT_UNCONFIGURED: &str = "This deployment isn't set up to mint credentials yet";

/// The mint refusal when a GitHub App installation must be reconnected.
pub const DETAIL_GITHUB_RECONNECT: &str = "GitHub App installation needs reconnect";

/// The mint refusal when the GitHub token exchange failed.
pub const DETAIL_MINT_FAILED: &str = "Credential mint failed";

/// The mint refusal when a connector's authorization has expired.
///
/// Provider-NEUTRAL on purpose. A Zoho refresh that failed must never tell a
/// runner to reconnect a GitHub App, which is what a shared sentence across the
/// two families would eventually do.
pub const DETAIL_CONNECTOR_RECONNECT: &str =
    "Connector authorization expired — reconnect the integration";

/// The mint refusal when a connector's token refresh failed.
pub const DETAIL_CONNECTOR_MINT_FAILED: &str = "Connector token refresh failed";

/// The refusal a message earns when its event came from no thread.
///
/// Written for the model that reads it through the `message` tool: it says
/// where the line should go instead.
pub const DETAIL_MESSAGE_NO_CHANNEL: &str =
    "This event came from no thread, so there is nowhere to post; say it in the answer instead";

/// The refusal a message past the per-run count earns.
pub const DETAIL_MESSAGE_LIMIT: &str = const_format::concatcp!(
    "This run already posted ",
    afd_wire::message_verb::MESSAGES_PER_RUN_MAX,
    " messages; put the rest in the answer"
);

/// The mint refusal when no approved grant covers the fleet and integration.
pub const DETAIL_GRANT_REQUIRED: &str =
    "No approved integration grant for this fleet and integration";

// One sentence per code, everywhere: two sentences for one code is the drift
// RULE UFS names, and a caller matching on the code cannot act on which
// handler it came from. The device-flow login and tenant api-key sentences
// live in `afd_auth::error`, and the memory operator surface's in
// `afd_memory::error::detail`, each beside the store that raises them.
