//! Creation's replay receipt, draft, candidate and first message commit together.

use super::read;
use crate::{
    Access, Drafts, Result,
    access::{self, Person},
    candidate,
    error::{ErrorKind, raise::query},
    message, model, replay, sql,
    types::{Accepted, CreateDraft, Created, EDITING, MAX_TITLE_CHARS, Source, SourcePin},
};
use afd_core::{error_code::AUTHORING_NOT_FOUND, id::Uuid7};
use afd_fleet_runtime::Definition;
use afd_observability::AuthoringStep;
use jiff::Timestamp;
use sqlx::{Connection as _, PgConnection};

const CONTEXT: &str = "create private draft";
const DEFAULT_TITLE: &str = "New fleet";
const INITIAL_COUNTER: i64 = 0;
const FIRST_REVISION: i64 = 1;

impl Drafts {
    /// Creates native private setup, optionally accepting its first message.
    ///
    /// # Errors
    /// Refuses unavailable sources/models and changed-body replay. Authorization
    /// precedes replay; any failure rolls back every new row.
    pub async fn create(
        &self,
        access: Access<'_>,
        request: CreateDraft,
        now: Timestamp,
    ) -> Result<Created> {
        let mut request = garde::Unvalidated::new(request).validate()?.into_inner();
        request.message = request.message.map(|text| message::normalize(&text));
        let request = garde::Unvalidated::new(request).validate()?;
        let digest = replay::digest(&*request)?;
        let mut connection = self.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        let scope = format!("{}:draft:create:{}", access.workspace, person.id);
        let existing = replay::find(
            &mut tx,
            &scope,
            replay::Resource::Draft,
            &request.operation_id,
            &person.id,
            &digest,
        )
        .await?;
        let (id, replayed) = if let Some(id) = existing {
            (id, true)
        } else {
            self.allocate(
                &mut tx,
                New {
                    access,
                    person: &person,
                    request: &request,
                    scope: &scope,
                    digest: &digest,
                    now,
                },
            )
            .await?
        };
        // A receipt outlives its draft, so a replayed key whose draft is
        // gone was accepted and then deleted: Gone, never a new draft.
        let locked = match read::lock(&mut tx, access, &person, &id).await {
            Ok(locked) => locked,
            Err(error) if replayed && error.code() == AUTHORING_NOT_FOUND => {
                return Err(ErrorKind::Gone.into());
            }
            Err(error) => return Err(error),
        };
        let draft = read::project(&mut tx, locked).await?;
        tx.commit().await.map_err(query(CONTEXT))?;
        if request.message.is_some() && !replayed {
            let step = AuthoringStep::MessageAccepted { draft_id: id };
            self.reached(access, step);
        }
        let accepted = request.message.as_ref().map(|_| Accepted {
            status: message::ACCEPTED,
            sequence: FIRST_REVISION.to_string(),
            replayed,
        });
        Ok(Created { draft, accepted })
    }

    async fn allocate(&self, connection: &mut PgConnection, new: New<'_>) -> Result<(Uuid7, bool)> {
        let captured = candidate::resolve(connection, new.access, new.person, new.request).await?;
        model::selected(
            connection,
            &new.person.owner,
            &new.request.authoring_model_ref,
        )
        .await?;
        let id = self.mint(new.now)?;
        let receipt = self.mint(new.now)?;
        let inserted = replay::reserve(
            connection,
            replay::Reservation {
                receipt_id: &receipt,
                workspace: new.access.workspace,
                person: new.person,
                scope: new.scope,
                operation: &new.request.operation_id,
                digest: new.digest,
                resource: &id,
            },
        )
        .await?;
        if !inserted {
            let existing = replay::find(
                connection,
                new.scope,
                replay::Resource::Draft,
                &new.request.operation_id,
                &new.person.id,
                new.digest,
            )
            .await?
            .ok_or(ErrorKind::Hidden)?;
            return Ok((existing, true));
        }
        write_draft(connection, &id, &new, &captured).await?;
        self.seed(connection, &id, &new, captured.definition.as_ref())
            .await?;
        Ok((id, false))
    }

    /// The new draft's first rows: the source's definition as candidate
    /// revision one, then the opening message, each under its own fresh id.
    async fn seed(
        &self,
        connection: &mut PgConnection,
        id: &Uuid7,
        new: &New<'_>,
        definition: Option<&Definition>,
    ) -> Result<()> {
        if let Some(definition) = definition {
            candidate::append(
                connection,
                candidate::Replacement {
                    id: &self.mint(new.now)?,
                    draft: id,
                    revision: FIRST_REVISION,
                    definition,
                },
            )
            .await?;
        }
        if let Some(text) = &new.request.message {
            message::insert(
                connection,
                message::Insertion {
                    id: &self.mint(new.now)?,
                    receipt: &self.mint(new.now)?,
                    workspace: new.access.workspace,
                    person: new.person,
                    draft: id,
                    sequence: FIRST_REVISION,
                    operation: &new.request.operation_id,
                    text,
                    model: &new.request.authoring_model_ref,
                },
            )
            .await?;
        }
        Ok(())
    }
}

/// The title a new chat starts with: the first line of its first request,
/// else of what its source describes, else [`DEFAULT_TITLE`].
///
/// Set once, at creation; a title the person edits later is theirs and nothing
/// here touches it again.
fn initial_title(message: Option<&str>, source: Option<&Definition>) -> String {
    message
        .and_then(first_line)
        .or_else(|| source.map(Definition::description).and_then(first_line))
        .map_or_else(
            || DEFAULT_TITLE.to_owned(),
            |line| line.chars().take(MAX_TITLE_CHARS).collect(),
        )
}

/// The first line of `text` with words in it, trimmed.
fn first_line(text: &str) -> Option<&str> {
    text.lines().map(str::trim).find(|line| !line.is_empty())
}

struct New<'a> {
    access: Access<'a>,
    person: &'a Person,
    request: &'a garde::Valid<CreateDraft>,
    scope: &'a str,
    digest: &'a str,
    now: Timestamp,
}

async fn write_draft(
    connection: &mut PgConnection,
    id: &Uuid7,
    new: &New<'_>,
    captured: &candidate::Captured,
) -> Result<()> {
    let pin = SourcePin::from(&new.request.source);
    let library_revision = match &new.request.source {
        Source::Library { revision_id, .. } => Some(revision_id.as_str()),
        Source::Blank | Source::Definition { .. } | Source::Fleet { .. } => None,
    };
    sqlx::query(sql::draft::CREATE)
        .bind(id.as_str())
        .bind(new.access.workspace.as_str())
        .bind(new.person.owner.as_str())
        .bind(new.person.id.as_str())
        .bind(initial_title(
            new.request.message.as_deref(),
            captured.definition.as_ref(),
        ))
        .bind(EDITING)
        .bind(model::tenant_model(&new.request.authoring_model_ref))
        .bind(pin.source_type())
        .bind(library_revision)
        .bind(captured.fleet_revision.as_ref().map(Uuid7::as_str))
        .bind(captured.instance_name.as_deref())
        .bind(captured.fleet.as_ref().map(Uuid7::as_str))
        .bind(INITIAL_COUNTER)
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}

#[cfg(test)]
#[expect(
    clippy::panic,
    reason = "fixed test fixtures fail loudly when their contract changes"
)]
mod tests {
    use afd_fleet_runtime::Definition;

    use super::{DEFAULT_TITLE, MAX_TITLE_CHARS, initial_title};

    /// A source setup describing itself as `description`.
    fn source(description: &str) -> Definition {
        let json = serde_json::json!({
            "schema_version": 1,
            "description": description,
            "instructions_md": "Run.",
            "config": {"name": "fixture", "x-agentsfleet": {
                "triggers": [], "tools": [], "budget": {"daily_nanos": "1000000000"}
            }},
        });
        Definition::parse(&json.to_string())
            .unwrap_or_else(|error| panic!("the fixture setup parses: {error:?}"))
    }

    #[test]
    fn a_new_chat_is_titled_by_its_request_then_its_source_then_the_default() {
        let described = source("Reviews pull requests.\nSecond line.");
        for (message, from, expected) in [
            (
                Some("  Triage my inbox\nthen reply"),
                Some(&described),
                "Triage my inbox",
            ),
            (Some("\n   \n"), Some(&described), "Reviews pull requests."),
            (None, Some(&described), "Reviews pull requests."),
            (None, None, DEFAULT_TITLE),
            (Some(""), None, DEFAULT_TITLE),
        ] {
            assert_eq!(initial_title(message, from), expected, "from {message:?}");
        }
    }

    #[test]
    fn a_long_first_line_is_cut_to_the_title_cap_at_a_character() {
        let long = "é".repeat(MAX_TITLE_CHARS + 5);
        let title = initial_title(Some(&long), None);
        assert_eq!(title.chars().count(), MAX_TITLE_CHARS);
        assert!(
            title.chars().all(|letter| letter == 'é'),
            "never split inside a character"
        );
    }
}
