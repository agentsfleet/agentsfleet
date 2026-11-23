//! Review binds immutable setup and resolved authority, separately from live readiness.

mod authority;
mod context;
mod credentials;
mod model;
mod readiness;
mod sql;
mod wallet;

use af_core::{clock, id::Uuid7};
use jiff::Timestamp;
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

pub(crate) use context::Context;

pub(super) struct Observed {
    checks: Vec<crate::types::Check>,
    versions: Vec<Value>,
    now: Timestamp,
    hold: Hold,
}

/// Whether a review holds the access it observes until its transaction ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hold {
    /// A write's review: the creator, the routing installs and the standing
    /// grants it read stay as read until the write that relies on them
    /// commits, so a revoke cannot land between the check and the write.
    Shared,
    /// A read's review, answered and discarded: it holds nothing, so it
    /// delays no install, grant or revoke.
    Nothing,
}

impl Hold {
    /// `held` for a write's review, `read` for a read's.
    const fn statement(self, held: &'static str, read: &'static str) -> &'static str {
        match self {
            Self::Shared => held,
            Self::Nothing => read,
        }
    }
}

impl Setup {
    /// Reads a creator-private draft and its current human review in one
    /// transaction that locks no draft, person, install or grant.
    ///
    /// Read committed rather than on one snapshot: the credential inspection
    /// opens its secrets `FOR SHARE` (`afd_vault`'s
    /// `select_shared_generations.sql`), which a read-only snapshot refuses.
    ///
    /// # Errors
    /// Hides foreign or revoked access and preserves datastore failures.
    pub async fn read_draft(&self, access: Access<'_>, id: &Uuid7) -> Result<Draft> {
        let mut connection = self.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::unlocked(&mut tx, access).await?;
        let draft = read::unlocked(&mut tx, access, &person, id).await?;
        let draft = self.review_draft_on(&mut tx, draft, Hold::Nothing).await?;
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(draft)
    }
    pub(crate) async fn review_draft_on(
        &self,
        connection: &mut PgConnection,
        row: read::DraftRow,
        hold: Hold,
    ) -> Result<Draft> {
        let review = match &row.head_revision_id {
            Some(id) => {
                let candidate = candidate::read(connection, &row.id, id).await?;
                Some(
                    build(
                        self,
                        connection,
                        &Context::from(&row),
                        &candidate,
                        clock::now(),
                        hold,
                    )
                    .await?,
                )
            }
            None => None,
        };
        let mut draft = read::project(connection, row).await?;
        draft.review = review;
        Ok(draft)
    }
}

pub(crate) async fn build(
    setup: &Setup,
    connection: &mut PgConnection,
    draft: &Context,
    candidate: &Candidate,
    now: Timestamp,
    hold: Hold,
) -> Result<Review> {
    let target = target(connection, draft).await?;
    let mut observed = Observed {
        checks: Vec::new(),
        versions: Vec::new(),
        now,
        hold,
    };
    let requirements = Requirements::of(&candidate.definition);
    authority::observe(connection, draft, &mut observed).await?;
    let reference = model::observe(connection, draft, candidate, &mut observed).await?;
    wallet::observe(setup.drafts.clock, connection, draft, &mut observed).await?;
    credentials::observe(
        setup,
        connection,
        draft,
        candidate,
        &requirements,
        &mut observed,
    )
    .await?;
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
    let revision: Option<Option<i64>> = sqlx::query_scalar(sql::TARGET)
        .bind(fleet)
        .bind(&draft.workspace_id)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(revision.flatten().map(|revision| revision.to_string()))
}
