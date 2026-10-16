//! Creation's replay receipt, draft, candidate and first message commit together.

use super::read;
use crate::{
    Access, Drafts, Result,
    access::{self, Person},
    candidate,
    error::{ErrorKind, raise::query},
    message, model, replay, sql,
    types::{Accepted, CreateDraft, Created, EDITING},
};
use afd_core::{clock::UnixMillis, error_code::AUTHORING_NOT_FOUND, id::Uuid7};
use afd_fleet_runtime::Definition;
use afd_observability::AuthoringStep;
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
        now: UnixMillis,
    ) -> Result<Created> {
        let mut request = garde::Unvalidated::new(request).validate()?.into_inner();
        request.message = request.message.map(|text| message::normalize(&text));
        let request = garde::Unvalidated::new(request).validate()?;
        let digest = replay::digest(&*request)?;
        let mut connection = self.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        let scope = format!("{}:draft:create:{}", access.workspace, person.id);
        let existing =
            replay::find(&mut tx, &scope, &request.operation_id, &person.id, &digest).await?;
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
        // Receipt reads do not lock ahead of the draft: deletion holds the
        // draft before updating receipts. Recheck a missing replay after a
        // concurrent purge commits so its accepted key returns Gone.
        let locked = match read::lock(&mut tx, access, &person, &id).await {
            Ok(locked) => locked,
            Err(error) => {
                if replayed && error.code() == AUTHORING_NOT_FOUND {
                    replay::find(&mut tx, &scope, &request.operation_id, &person.id, &digest)
                        .await?;
                }
                return Err(error);
            }
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
                now: new.now,
            },
        )
        .await?;
        if !inserted {
            let existing = replay::find(
                connection,
                new.scope,
                &new.request.operation_id,
                &new.person.id,
                new.digest,
            )
            .await?
            .ok_or(ErrorKind::Hidden)?;
            return Ok((existing, true));
        }
        write_draft(
            connection,
            &id,
            &new,
            captured.fleet.as_ref(),
            captured.instance_name.as_deref(),
        )
        .await?;
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
                    now: new.now,
                },
            )
            .await?;
        }
        if let Some(text) = &new.request.message {
            message::insert(
                connection,
                message::Insertion {
                    id: &self.mint(new.now)?,
                    draft: id,
                    sequence: FIRST_REVISION,
                    operation: &new.request.operation_id,
                    text,
                    model: &new.request.authoring_model_ref,
                    now: new.now,
                },
            )
            .await?;
        }
        Ok(())
    }
}

struct New<'a> {
    access: Access<'a>,
    person: &'a Person,
    request: &'a garde::Valid<CreateDraft>,
    scope: &'a str,
    digest: &'a str,
    now: UnixMillis,
}

async fn write_draft(
    connection: &mut PgConnection,
    id: &Uuid7,
    new: &New<'_>,
    fleet: Option<&Uuid7>,
    instance_name: Option<&str>,
) -> Result<()> {
    sqlx::query(sql::draft::CREATE)
        .bind(id.as_str())
        .bind(new.access.workspace.as_str())
        .bind(new.person.owner.as_str())
        .bind(new.person.id.as_str())
        .bind(new.request.operation_id.as_str())
        .bind(new.digest)
        .bind(DEFAULT_TITLE)
        .bind(EDITING)
        .bind(&new.request.authoring_model_ref)
        .bind(INITIAL_COUNTER)
        .bind(fleet.map(Uuid7::as_str))
        .bind(new.now.as_millis())
        .bind(
            serde_json::to_string(&crate::types::SourcePin::from(&new.request.source))
                .map_err(crate::error::raise::stored)?,
        )
        .bind(instance_name)
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}
