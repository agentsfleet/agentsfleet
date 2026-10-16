//! Review binds immutable setup and resolved authority, separately from live readiness.

mod authority;
mod context;
mod credentials;
mod model;
mod readiness;
mod wallet;

use afd_core::{clock, id::Uuid7};
use serde_json::{Value, json};
use sqlx::{Connection as _, PgConnection};

use crate::{
    Access, Result, Setup, access, candidate,
    draft::read,
    error::raise::query,
    replay,
    types::{Candidate, Draft, Effective, Requirements, Review},
};

const CONTEXT: &str = "review current native setup";
const TARGET: &str =
    "SELECT setup_version FROM core.fleets WHERE id=$1::uuid AND workspace_id=$2::uuid";

pub(crate) use context::{Context, Subject};
pub(crate) use readiness::review_action;

pub(super) struct Observed {
    checks: Vec<crate::types::Check>,
    versions: Vec<Value>,
    now: i64,
}

impl Setup {
    /// Reads a creator-private draft and its current human review in one transaction.
    ///
    /// # Errors
    /// Hides foreign or revoked access and preserves datastore failures.
    pub async fn read_draft(&self, access: Access<'_>, id: &Uuid7) -> Result<Draft> {
        let mut connection = self.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        let locked = read::lock(&mut tx, access, &person, id).await?;
        let review = match &locked.current_candidate_id {
            Some(id) => {
                let candidate = candidate::read(&mut tx, &locked.id, id).await?;
                Some(
                    build(
                        self,
                        &mut tx,
                        &Context::from(&locked),
                        &candidate,
                        clock::now().as_millis(),
                    )
                    .await?,
                )
            }
            None => None,
        };
        let mut draft = read::project(&mut tx, locked).await?;
        draft.review = review;
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(draft)
    }
}

pub(crate) async fn build(
    setup: &Setup,
    connection: &mut PgConnection,
    draft: &Context,
    candidate: &Candidate,
    now: i64,
) -> Result<Review> {
    let mut observed = Observed {
        checks: Vec::new(),
        versions: Vec::new(),
        now,
    };
    let requirements = Requirements::of(&candidate.definition);
    authority::observe(connection, draft, &mut observed).await?;
    let reference = model::observe(connection, draft, candidate, &mut observed).await?;
    wallet::observe(connection, &draft.tenant_id, &mut observed).await?;
    credentials::observe(
        setup,
        connection,
        draft,
        candidate,
        &requirements,
        &mut observed,
    )
    .await?;
    let target = target(connection, draft).await?;
    observed.base(draft, candidate, target.as_deref());
    let effective = effective(draft, candidate, reference, target);
    let review_digest = replay::digest(&json!({"purpose":"setup_review", "version":1,
        "candidate_id":candidate.id, "candidate_revision":candidate.revision,
        "subject":draft.subject, "workspace_id":draft.workspace_id, "creator_id":draft.creator_id,
        "effective":effective, "source":draft.source_pin,
        "policy_semantics":candidate.policy_semantics, "access_versions":observed.versions}))?;
    Ok(Review {
        effective,
        requirements,
        readiness: observed.checks,
        review_digest,
        policy_semantics: candidate.policy_semantics.clone(),
        observed_at: now,
    })
}

/// What the setup will run as: the candidate's definition under the draft's
/// instance name (the definition's own name when the draft names none), the
/// resolved model, and the revisions it was read from and would replace.
fn effective(
    draft: &Context,
    candidate: &Candidate,
    reference: Option<String>,
    target: Option<String>,
) -> Effective {
    Effective {
        definition: candidate.definition.clone(),
        instance_name: draft
            .instance_name
            .clone()
            .unwrap_or_else(|| candidate.definition.runtime().name().as_str().to_owned()),
        runtime_model_ref: reference,
        source_revision: draft
            .source_pin
            .as_ref()
            .and_then(crate::types::SourcePin::revision),
        target_setup_revision: target,
    }
}

async fn target(connection: &mut PgConnection, draft: &Context) -> Result<Option<String>> {
    let Some(fleet) = draft.target() else {
        return Ok(None);
    };
    let revision: Option<Option<i64>> = sqlx::query_scalar(TARGET)
        .bind(fleet.as_str())
        .bind(draft.workspace_id.as_str())
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(revision.flatten().map(|revision| revision.to_string()))
}
