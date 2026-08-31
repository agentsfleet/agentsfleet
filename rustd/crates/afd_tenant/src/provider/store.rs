//! The tenant's provider selection over the api-role pool.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_db::Db;
use sqlx::Row as _;

use super::{Posture, SecretKind, Selection};
use crate::sql::provider as sql;
use crate::{Result, error};

/// The statement label a failed read is reported under.
const CONTEXT_READ: &str = "tenant provider read";

/// The statement label a failed write is reported under.
const CONTEXT_WRITE: &str = "tenant provider write";

/// The statement label a failed credential probe is reported under.
const CONTEXT_SECRET: &str = "tenant provider credential probe";

/// The tenant's provider selection, read and written.
///
/// Holds the api-role pool for the reason [`Preferences`](crate::preference::Preferences)
/// does: every one of these is on a dashboard request path, and a request-path
/// read sharing a pool with background work waits behind it.
#[derive(Debug, Clone)]
pub struct Providers {
    database: Db,
}

impl Providers {
    /// A provider store over `database`.
    #[must_use]
    pub const fn new(database: Db) -> Self {
        Self { database }
    }

    /// This tenant's selection, or `None` if it never configured one.
    ///
    /// `None` and platform mode are DIFFERENT answers and the dashboard renders
    /// them differently: no row means never configured, a platform-mode row
    /// means explicitly reset. Collapsing them would lose the distinction the
    /// write path exists to record.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a stored row whose two
    /// posture columns do not describe a posture.
    pub async fn selection(&self, tenant: &Uuid7) -> Result<Option<Selection>> {
        let mut connection = self.database.acquire().await?;
        let row = sqlx::query(sql::SELECT_SELECTION)
            .bind(tenant.as_str())
            .fetch_optional(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_READ))?;

        let Some(row) = row else { return Ok(None) };
        let unreadable = error::query(CONTEXT_READ);
        let mode: String = row.try_get(0).map_err(&unreadable)?;
        let secret_ref: Option<String> = row.try_get(4).map_err(&unreadable)?;

        Ok(Some(Selection {
            // `?` lifts StoredPosture into this crate's Error through the
            // error_lifts! declaration beside ErrorKind — no arm here.
            posture: Posture::from_columns(&mode, secret_ref.as_deref())?,
            provider: row.try_get::<String, _>(1).map_err(&unreadable)?.into(),
            model: row.try_get::<String, _>(2).map_err(&unreadable)?.into(),
            context_cap_tokens: cap_from_row(&row, &unreadable)?,
            configured_at: UnixMillis::from_millis(row.try_get(5).map_err(&unreadable)?),
            updated_at: UnixMillis::from_millis(row.try_get(6).map_err(&unreadable)?),
        }))
    }

    /// Writes the selection, last-write-wins on the tenant's single row.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn upsert(
        &self,
        tenant: &Uuid7,
        selection: &Selection,
        now: UnixMillis,
    ) -> Result<()> {
        let mut connection = self.database.acquire().await?;
        sqlx::query(sql::UPSERT_SELECTION)
            .bind(tenant.as_str())
            .bind(selection.posture.mode())
            .bind(&*selection.provider)
            .bind(&*selection.model)
            .bind(i32::try_from(selection.context_cap_tokens).unwrap_or(i32::MAX))
            .bind(selection.posture.secret_ref())
            .bind(now.as_millis())
            .execute(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_WRITE))?;

        Ok(())
    }

    /// What kind of credential this workspace holds under `name`.
    ///
    /// Reads the vault's non-secret metadata and opens nothing — see
    /// [`sql::SELECT_SECRET_SHAPE`](crate::sql::provider::SELECT_SECRET_SHAPE).
    /// The two refusals the write ladder distinguishes come out of one round
    /// trip: [`SecretKind::Absent`] is rung two, [`SecretKind::NotAProviderKey`]
    /// is rung three.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn secret_kind(&self, workspace: &Uuid7, name: &str) -> Result<SecretKind> {
        let mut connection = self.database.acquire().await?;
        let row = sqlx::query(sql::SELECT_SECRET_SHAPE)
            .bind(workspace.as_str())
            .bind(name)
            .fetch_optional(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_SECRET))?;

        let Some(row) = row else {
            return Ok(SecretKind::Absent);
        };
        let unreadable = error::query(CONTEXT_SECRET);
        let provider: Option<String> = row.try_get(0).map_err(&unreadable)?;
        let has_key: Option<bool> = row.try_get(1).map_err(&unreadable)?;

        Ok(SecretKind::of(provider.as_deref(), has_key))
    }
}

/// The context cap, narrowed from the column's signed width.
///
/// A negative cap is corruption of the same class a bad posture is, so it
/// reports through the same type and lifts through the same `From`.
fn cap_from_row(
    row: &sqlx::postgres::PgRow,
    unreadable: &impl Fn(sqlx::Error) -> crate::Error,
) -> Result<u32> {
    let stored: i32 = row.try_get(3).map_err(unreadable)?;
    if stored.is_negative() {
        // Tested by the sign rather than by `try_from`, so the refusal keeps
        // the value that provoked it — a discarded `TryFromIntError` says only
        // "out of range", which the caller already knew.
        return Err(super::StoredPosture::ContextCapOutOfRange { stored }.into());
    }
    Ok(stored.unsigned_abs())
}
