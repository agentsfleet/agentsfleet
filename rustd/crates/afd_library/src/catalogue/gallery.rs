//! `GET /v1/workspaces/{workspace_id}/fleet-libraries` — the workspace gallery.
//!
//! The platform catalogue unioned with this workspace's own entries, and
//! nothing from another workspace. [`super::store`] administers the platform
//! half for an operator; this reads both halves as one page for a tenant.
//!
//! # The tier is a rank, and the rank never reaches the wire
//!
//! Merging two tables needs a total order across both, and the tier
//! participates in it — platform entries sort before tenant entries at the same
//! instant. Ordering on the LABEL would make that alphabetical coincidence
//! rather than intent, and would invert the day a third tier named `curated`
//! appears. [`Tier`] states the intent as a number and renders as a label, so
//! the comparison says what it means and a bare rank cannot leak into a
//! response body: [`Tier::from_rank`] answers `None` for anything outside the
//! enum, and a row carrying one fails the read.
//!
//! # `visibility` on this wire is the tier, not the publication state
//!
//! Two different facts share one field name across two surfaces, which is worth
//! reading once. `core.fleet_library.visibility` holds `draft`/`public` — an
//! operator's publication state, and the platform arm's own filter. The gallery
//! card's `visibility` holds `platform`/`tenant` — which library the row came
//! from. The owner declined renaming the wire field to `tier`, because
//! `docs/REST_API_DESIGN_GUIDELINES.md` §9 forbids renaming a shipped v1 field,
//! so the collision stays and this note is the reconciliation.

mod sql;

use afd_core::id::Uuid7;
use serde_json::Value;
use sqlx::Row as _;
use sqlx::postgres::PgRow;

use super::{Libraries, LibraryRequirements, VISIBILITY_PUBLIC};
use crate::{Error, Result};

/// The context a failed gallery read reports under.
const CONTEXT_GALLERY: &str = "list a workspace's fleet libraries";

/// The persisted and wire spelling of the platform tier.
const LABEL_PLATFORM: &str = "platform";

/// The persisted and wire spelling of the tenant tier.
const LABEL_TENANT: &str = "tenant";

/// Which library a gallery row came from.
///
/// The numeric rank is the sort position, not an encoding of the name. Both
/// spellings are declared once so [`Tier::from_label`] and [`Tier::label`]
/// cannot drift into disagreeing — a parse that accepts a spelling the renderer
/// never emits is a round trip that silently loses rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    /// The platform catalogue, curated by an operator.
    Platform,
    /// This workspace's own entries, onboarded at runtime.
    Tenant,
}

impl Tier {
    /// The sort position, which the merged order compares on.
    #[must_use]
    pub const fn rank(self) -> i32 {
        match self {
            Self::Platform => 0,
            Self::Tenant => 1,
        }
    }

    /// The persisted and wire spelling.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Platform => LABEL_PLATFORM,
            Self::Tenant => LABEL_TENANT,
        }
    }

    /// The tier a persisted sort rank names, if this build knows it.
    ///
    /// `None` rather than a default, and the default is the dangerous half: a
    /// rank silently becoming `Platform` would leak entries from a library the
    /// caller cannot read. A rank the projection cannot name fails the read
    /// instead of reaching a response body as a bare number.
    #[must_use]
    pub const fn from_rank(rank: i32) -> Option<Self> {
        match rank {
            0 => Some(Self::Platform),
            1 => Some(Self::Tenant),
            _unknown => None,
        }
    }

    /// The tier a spelling names, if this build knows it.
    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        [Self::Platform, Self::Tenant]
            .into_iter()
            .find(|tier| tier.label() == label)
    }
}

/// Where a later gallery page resumes from.
///
/// All three parts of the compound order, because all three are needed to place
/// a row in it. The token a client holds carries more — the workspace and the
/// page size it was issued under — but binding a cursor to its query is a
/// decision only the handler can make and check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    /// The boundary row's creation instant.
    pub created_at_ms: i64,
    /// Which library it came from.
    pub tier: Tier,
    /// Its identifier, compared bytewise.
    pub id: String,
}

/// One gallery card.
///
/// Everything here is rendered. There is no field for a skill body, a support
/// file, or an object-store key — a read cannot leak bundle content because the
/// struct it would leak through has nowhere to put it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryEntry {
    /// The entry's identifier: a slug on the platform side, a UUID on the
    /// tenant side. Opaque to a caller either way.
    pub id: String,
    /// The display name.
    pub name: String,
    /// The summary from the bundle's own frontmatter.
    pub description: String,
    /// Which library it came from — the card's `visibility`.
    pub tier: Tier,
    /// The repository or template it was onboarded from.
    pub source_ref: String,
    /// When it was onboarded, in epoch milliseconds.
    pub created_at_ms: i64,
    /// What the bundle declares it needs. Names, never values.
    pub requirements: LibraryRequirements,
    /// The per-credential "why this fleet needs it" copy, keyed by name.
    ///
    /// Empty for a tenant entry: that table derives no reasons, and the arm
    /// projects an empty object rather than a null so both halves of the union
    /// carry the same type.
    pub required_credentials_reasons: Value,
}

/// One page of the gallery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GalleryPage {
    /// The cards, newest first, platform before tenant within an instant.
    pub items: Vec<SummaryEntry>,
    /// Where the next page resumes, or nothing on the last one.
    pub next: Option<Position>,
}

impl Libraries {
    /// One page of `workspace`'s gallery.
    ///
    /// `after` is the decoded boundary from the caller's cursor, already checked
    /// against the workspace in the path and the requested limit. This trusts
    /// it, because only the handler can perform that comparison — and the
    /// workspace the read is SCOPED to is always the path's, never the cursor's.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, a requirements blob this
    /// daemon cannot read, and a tier rank outside the ones it knows.
    pub async fn gallery(
        &self,
        workspace: &Uuid7,
        limit: u32,
        after: Option<&Position>,
    ) -> Result<GalleryPage> {
        let fetch = i64::from(limit) + 1;
        let statement = match after {
            None => sqlx::query(sql::FIRST_PAGE)
                .bind(VISIBILITY_PUBLIC)
                .bind(workspace.as_str())
                .bind(fetch),
            Some(position) => sqlx::query(sql::PAGE_AFTER)
                .bind(VISIBILITY_PUBLIC)
                .bind(workspace.as_str())
                .bind(position.created_at_ms)
                .bind(position.tier.rank())
                .bind(position.id.as_str())
                .bind(fetch),
        };

        let mut connection = self.0.acquire().await?;
        let rows = statement
            .fetch_all(&mut *connection)
            .await
            .map_err(Error::database(CONTEXT_GALLERY))?;

        let has_more = rows.len() > limit as usize;
        let items: Vec<SummaryEntry> = rows
            .iter()
            .take(limit as usize)
            .map(decode)
            .collect::<Result<_>>()?;

        // From the last SERVED card, never the over-fetched one: the seek is
        // strict, so the next page has to resume after what the caller saw.
        let next = has_more.then(|| items.last().map(position_of)).flatten();
        Ok(GalleryPage { items, next })
    }
}

/// Where a later page resumes after `entry`.
fn position_of(entry: &SummaryEntry) -> Position {
    Position {
        created_at_ms: entry.created_at_ms,
        tier: entry.tier,
        id: entry.id.clone(),
    }
}

/// Reads one merged row into a card.
///
/// Positional, matching every other read in this workspace: the statement's
/// projection and this function are one contract, and reading by name would
/// hide a projection that had drifted out of order.
fn decode(row: &PgRow) -> Result<SummaryEntry> {
    let unreadable = Error::database(CONTEXT_GALLERY);
    let id: String = row.try_get(0).map_err(&unreadable)?;
    let name: String = row.try_get(1).map_err(&unreadable)?;
    let description: String = row.try_get(2).map_err(&unreadable)?;
    let source_ref: String = row.try_get(3).map_err(&unreadable)?;
    let created_at_ms: i64 = row.try_get(4).map_err(&unreadable)?;
    let credentials: String = row.try_get(5).map_err(&unreadable)?;
    let tools: String = row.try_get(6).map_err(&unreadable)?;
    let hosts: String = row.try_get(7).map_err(&unreadable)?;
    let reasons: String = row.try_get(8).map_err(&unreadable)?;
    let trigger_present: bool = row.try_get(9).map_err(&unreadable)?;
    let rank: i32 = row.try_get(10).map_err(&unreadable)?;

    Ok(SummaryEntry {
        id,
        name,
        description,
        // The rank is mapped back to its tier HERE, so a rank this build cannot
        // name is a loud failure rather than a bare number in a response body.
        tier: Tier::from_rank(rank)
            .ok_or_else(|| Error::database(CONTEXT_GALLERY)(unknown(rank)))?,
        source_ref,
        created_at_ms,
        requirements: LibraryRequirements::new(
            serde_json::from_str(&credentials)?,
            serde_json::from_str(&tools)?,
            serde_json::from_str(&hosts)?,
            trigger_present,
        ),
        required_credentials_reasons: serde_json::from_str(&reasons)?,
    })
}

/// The failure a tier rank outside the enum reports.
///
/// Spelled as a decode error rather than a bespoke variant: the row IS
/// unreadable by this build, which is exactly what the column-type failures
/// beside it mean, and a caller acts on both the same way.
fn unknown(rank: i32) -> sqlx::Error {
    sqlx::Error::Decode(format!("tier_rank {rank} names no tier this build serves").into())
}

#[cfg(test)]
mod tests {
    use super::{LABEL_PLATFORM, LABEL_TENANT, Tier};

    #[test]
    fn the_ranks_order_platform_before_tenant() {
        // The merged order depends on this and nothing else does. Ordering on
        // the label would make it alphabetical coincidence, which is the same
        // answer today and the wrong one the day a `curated` tier appears.
        assert!(Tier::Platform.rank() < Tier::Tenant.rank());
    }

    #[test]
    fn every_rank_and_label_round_trips() {
        // The drift this pins: a parse that accepts a spelling the renderer
        // never emits, or a rank the projection cannot map back, both lose rows
        // silently rather than loudly.
        for tier in [Tier::Platform, Tier::Tenant] {
            assert_eq!(Tier::from_rank(tier.rank()), Some(tier));
            assert_eq!(Tier::from_label(tier.label()), Some(tier));
        }
        assert_eq!(Tier::Platform.label(), LABEL_PLATFORM);
        assert_eq!(Tier::Tenant.label(), LABEL_TENANT);
    }

    #[test]
    fn a_rank_or_label_this_build_does_not_know_is_refused() {
        // Never defaulted to `Platform`: that would leak entries from a library
        // the caller cannot read, and it would look like a working page.
        for rank in [-1, 2, i32::MAX, i32::MIN] {
            assert_eq!(Tier::from_rank(rank), None, "rank {rank}");
        }
        for label in ["", "curated", "PLATFORM", "platform "] {
            assert_eq!(Tier::from_label(label), None, "label {label:?}");
        }
    }
}
