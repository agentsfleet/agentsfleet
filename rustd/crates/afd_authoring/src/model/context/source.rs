//! Complete persisted exchanges, paged independently of conversation length.
use crate::lease::guard::Attempt;
use afd_context::{Group, HistorySource, Page, PortFuture};
use sqlx::PgConnection;

pub(super) struct Source<'a> {
    connection: &'a mut PgConnection,
    attempt: &'a Attempt,
    pub(super) fingerprint: super::snapshot::Fingerprint,
}

impl<'a> Source<'a> {
    pub(super) fn new(connection: &'a mut PgConnection, attempt: &'a Attempt) -> Self {
        Self {
            connection,
            attempt,
            fingerprint: super::snapshot::Fingerprint::default(),
        }
    }
}

#[derive(sqlx::FromRow)]
struct Row {
    sequence: i64,
    user_text: String,
    answer_text: Option<String>,
}

impl HistorySource for Source<'_> {
    fn page(&mut self, sequence: u64, limit: usize) -> PortFuture<'_, Page> {
        Box::pin(async move {
            let after = i64::try_from(sequence).map_err(afd_context::Error::boundary)?;
            let sql_limit = i64::try_from(limit).map_err(afd_context::Error::boundary)?;
            let rows: Vec<Row> = sqlx::query_as(include_str!("history.sql"))
                .bind(self.attempt.draft_id.as_str())
                .bind(after)
                .bind(self.attempt.message_sequence)
                .bind(sql_limit)
                .fetch_all(&mut *self.connection)
                .await
                .map_err(afd_context::Error::boundary)?;
            let has_more = rows.len() == limit;
            let groups = rows
                .into_iter()
                .map(|row| {
                    Ok(Group {
                        sequence: u64::try_from(row.sequence)
                            .map_err(afd_context::Error::boundary)?,
                        text: serde_json::json!({"user":row.user_text,"assistant":row.answer_text})
                            .to_string(),
                    })
                })
                .collect::<afd_context::Result<Vec<_>>>()?;
            self.fingerprint.include(&groups, has_more);
            Ok(Page { groups, has_more })
        })
    }
}
