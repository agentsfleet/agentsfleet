//! The library read and the row write — the two statements an install runs
//! before Redis is reached.
//!
//! # Why the name retry lives here
//!
//! A name the CALLER chose and a name the BUNDLE carried collide differently.
//! An explicit collision is an honest conflict they must see: renaming what
//! somebody typed is worse than refusing it. A defaulted collision is the
//! second-install-from-one-template case, and reporting it would name a
//! conflict on a value the caller never chose — so it is re-drawn with a
//! suffix, three times, and only then reported.
//!
//! The Zig expresses that as a three-argument classifier returning a
//! three-armed enum. Here it is the shape of the loop: an explicit name has no
//! retry branch to take, because [`Naming::Chosen`] carries no attempts.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use sqlx::Row as _;

use crate::error::{self, ErrorKind, Result};
use crate::{FleetStatus, Fleets, sql};

use super::authored::Authored;
use super::{Install, LibrarySource};

/// How many names a DEFAULTED install draws before reporting the collision.
///
/// The suffix space is a thousand, so three losing draws is evidence of
/// something broken — an entropy source gone flat — rather than bad luck.
const NAME_ATTEMPTS: u32 = 3;

/// The exclusive bound on a drawn suffix, keeping it three digits.
const SUFFIX_SPACE: u32 = 1000;

/// The visibility a platform entry must carry to be installable.
const VISIBILITY_PUBLIC: &str = "public";

/// The index arbitrating one name inside one workspace.
///
/// Classification is by EXACT constraint and not by SQLSTATE alone: the table
/// carries other unique indexes, and an identifier collision is not a fact
/// about the NAME. A rename landing on one side turns a duplicate-name conflict
/// into a 500, which is the regression the Zig comment records.
const NAME_CONSTRAINT: &str = "uq_fleets_workspace_id_name";

/// Postgres's unique-violation SQLSTATE.
const UNIQUE_VIOLATION: &str = "23505";

/// The contexts a failed statement on this path reports under.
const CONTEXT_LIBRARY: &str = "resolve install source";
const CONTEXT_INSERT: &str = "insert fleet row";

/// A library entry, resolved for install.
#[derive(Debug)]
pub(super) struct Entry {
    /// The authored `SKILL.md`.
    pub(super) skill_markdown: String,
    /// The authored `TRIGGER.md`, where the bundle carried one.
    pub(super) trigger_markdown: Option<String>,
    /// What a runner materialises support files from.
    pub(super) content_hash: String,
}

/// How this install's name was decided, and therefore how a collision reads.
enum Naming {
    /// The caller typed it. A collision is theirs to see.
    Chosen(String),
    /// The bundle carried it. A collision is re-drawn, up to `left` more times.
    Drawn { base: String, left: u32 },
}

impl Fleets {
    /// Reads the library entry this install draws from.
    ///
    /// # Errors
    /// Refuses an id naming nothing installable in this workspace — an
    /// unpublished platform entry and another workspace's tenant entry both
    /// resolve nothing, because the predicate is the check. Reports a datastore
    /// that would not answer.
    pub(super) async fn resolve(
        &self,
        connection: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
        workspace: &Uuid7,
        source: &LibrarySource<'_>,
    ) -> Result<Entry> {
        let query = match source {
            LibrarySource::Platform(slug) => sqlx::query(sql::SELECT_PLATFORM_INSTALL)
                .bind(*slug)
                .bind(VISIBILITY_PUBLIC),
            LibrarySource::Tenant(id) => sqlx::query(sql::SELECT_TENANT_INSTALL)
                .bind(id.as_str())
                .bind(workspace.as_str()),
        };
        let found = query
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_LIBRARY))?;
        let row = found.ok_or_else(|| crate::Error::from(ErrorKind::LibraryEntryMissing))?;

        let unreadable = error::query(CONTEXT_LIBRARY);
        Ok(Entry {
            skill_markdown: row.try_get(0).map_err(&unreadable)?,
            trigger_markdown: row.try_get(1).map_err(&unreadable)?,
            content_hash: row.try_get(2).map_err(&unreadable)?,
        })
    }

    /// Writes the row, re-drawing a DEFAULTED name that loses its race.
    ///
    /// Answers the name the row was actually stored under, which is not always
    /// the one that went in.
    ///
    /// # Errors
    /// Refuses a chosen name this workspace already holds, and a defaulted one
    /// that lost every draw. Reports a datastore that would not answer.
    pub(super) async fn insert_with_retry(
        &self,
        connection: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
        workspace: &Uuid7,
        id: &Uuid7,
        authored: &Authored,
        request: &Install<'_>,
        now: UnixMillis,
    ) -> Result<String> {
        let mut naming = match request.name.as_ref() {
            Some(chosen) => Naming::Chosen(chosen.as_str().to_owned()),
            None => Naming::Drawn {
                base: authored.trigger.config().name().as_str().to_owned(),
                left: NAME_ATTEMPTS,
            },
        };
        loop {
            let candidate = naming.candidate();
            match self
                .insert(connection, workspace, id, &candidate, authored, now)
                .await
            {
                Ok(()) => return Ok(candidate),
                Err(source) if is_name_conflict(&source) => match naming.redraw(&self.entropy)? {
                    Some(next) => naming = next,
                    None => return Err(ErrorKind::NameExists.into()),
                },
                Err(source) => return Err(error::query(CONTEXT_INSERT)(source)),
            }
        }
    }

    /// One insert, answering the driver's own error for the caller to classify.
    ///
    /// The row is born [`FleetStatus::Installing`]. Nothing can lease it until
    /// the caller's pipeline flips it, which is what makes the stream guarantee
    /// enforceable rather than merely intended.
    async fn insert(
        &self,
        connection: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
        workspace: &Uuid7,
        id: &Uuid7,
        name: &str,
        authored: &Authored,
        now: UnixMillis,
    ) -> std::result::Result<(), sqlx::Error> {
        sqlx::query(sql::INSERT_FLEET)
            .bind(id.as_str())
            .bind(workspace.as_str())
            .bind(name)
            .bind(&authored.entry.skill_markdown)
            .bind(&authored.trigger_markdown)
            .bind(authored.trigger.config_json())
            .bind(FleetStatus::Installing.as_str())
            .bind(authored.required_tags())
            .bind(&authored.entry.content_hash)
            .bind(snapshot_key(&authored.entry.content_hash))
            .bind(now.as_millis())
            .execute(connection.as_mut())
            .await
            .map(|_written| ())
    }
}

impl Naming {
    /// The name this attempt writes.
    fn candidate(&self) -> String {
        match self {
            Self::Chosen(name) | Self::Drawn { base: name, .. } => name.clone(),
        }
    }

    /// The next naming to try, or `None` when the collision is the answer.
    ///
    /// A chosen name never redraws — the arm simply does not exist for it,
    /// which is the type doing what the Zig's `name_explicit` boolean does at
    /// runtime.
    fn redraw(&self, entropy: &afd_crypto::entropy::Entropy) -> Result<Option<Self>> {
        match self {
            Self::Chosen(_typed) => Ok(None),
            Self::Drawn { left: 0, .. } => Ok(None),
            Self::Drawn { base, left } => {
                let mut bytes = [0u8; 4];
                entropy.fill(&mut bytes)?;
                let suffix = u32::from_be_bytes(bytes) % SUFFIX_SPACE;
                Ok(Some(Self::Drawn {
                    base: format!("{base}-{suffix:03}"),
                    left: left - 1,
                }))
            }
        }
    }
}

/// Where a bundle's snapshot is stored, derived from its content hash.
///
/// `importer.snapshotKey`'s layout, and derived rather than stored for the
/// reason a documentation link is: a key built from the hash can never name a
/// different bundle's snapshot.
fn snapshot_key(content_hash: &str) -> String {
    format!("fleet-bundles/{content_hash}.tar.zst")
}

/// Tells a lost name race apart from a broken statement.
fn is_name_conflict(source: &sqlx::Error) -> bool {
    source.as_database_error().is_some_and(|failure| {
        failure.code().is_some_and(|code| code == UNIQUE_VIOLATION)
            && failure.constraint() == Some(NAME_CONSTRAINT)
    })
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the restriction set is for the daemon"
    )]
    use super::{NAME_ATTEMPTS, Naming, SUFFIX_SPACE, snapshot_key};
    use afd_crypto::entropy::Entropy;

    #[test]
    fn a_chosen_name_is_never_re_drawn() {
        // Renaming what somebody typed is worse than refusing it, and the type
        // is what makes that unmissable: there is no attempts field to exhaust.
        let chosen = Naming::Chosen("payments-bot".to_owned());
        let next = chosen.redraw(&Entropy::new()).expect("no entropy failure");

        assert!(next.is_none(), "an explicit collision is the answer");
    }

    #[test]
    fn a_drawn_name_re_draws_a_bounded_number_of_times() {
        let entropy = Entropy::new();
        let mut naming = Naming::Drawn {
            base: "daily-digest".to_owned(),
            left: NAME_ATTEMPTS,
        };
        for _draw in 0..NAME_ATTEMPTS {
            naming = naming
                .redraw(&entropy)
                .expect("no entropy failure")
                .expect("a draw remains");
        }

        assert!(
            naming
                .redraw(&entropy)
                .expect("no entropy failure")
                .is_none(),
            "the bound stops the loop rather than retrying forever"
        );
    }

    #[test]
    fn a_re_drawn_name_keeps_its_base_and_takes_a_three_digit_tail() {
        let naming = Naming::Drawn {
            base: "daily-digest".to_owned(),
            left: 1,
        };
        let redrawn = naming
            .redraw(&Entropy::new())
            .expect("no entropy failure")
            .expect("a draw remains")
            .candidate();

        let (base, tail) = redrawn.rsplit_once('-').expect("a suffix was appended");
        assert_eq!(base, "daily-digest");
        assert_eq!(tail.len(), 3, "three digits, so the slug stays in bounds");
        assert!(tail.parse::<u32>().is_ok_and(|value| value < SUFFIX_SPACE));
    }

    #[test]
    fn a_snapshot_key_names_only_its_own_bundle() {
        assert_ne!(snapshot_key("abc123"), snapshot_key("abc124"));
        assert!(snapshot_key("abc123").contains("abc123"));
    }
}
