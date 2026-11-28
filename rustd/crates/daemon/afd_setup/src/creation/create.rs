//! A person's first message: the fleet, its thread and its first turn, in one
//! transaction.
//!
//! The replay receipt is written first, under `<workspace>:fleet:create:<person>`,
//! so callers repeating one operation queue on its key: the first creates, and
//! every later one answers that fleet. The wake and the workspace's fleet
//! notice follow the commit; a daemon that dies between the two leaves a
//! queued turn, which the next boot's gap pass assigns.

use af_core::id::Uuid7;
use af_fleet_runtime::FleetName;
use af_wire::turn::TurnKind;
use afd_api_wire::fleet::CreateFleetRequest;
use afd_turns::{IncomingTurn, Key, Producer, Reply};
use jiff::Timestamp;
use sqlx::{Connection as _, PgConnection};

use super::{ACTIVE, CONTEXT, Creations, INITIAL_VERSION, model, read, sql};
use crate::{
    Access, Result,
    access::{self, Person},
    error::{
        ErrorKind,
        raise::{absent, query},
    },
    replay,
    types::CreatedFleet,
};

/// What a name the person did not choose starts with.
const DERIVED_NAME_PREFIX: &str = "fleet-";
/// How many trailing hex digits of the fleet's id a derived name carries:
/// the random tail of a version 7 identifier.
const DERIVED_NAME_DIGITS: usize = 12;
/// The operator line a committed create writes.
const EVENT_CREATED: &str = "fleet_created";

/// What one try at the create came to.
enum Attempt {
    /// The fleet exists: this call's, or an earlier call's with the same key.
    Done(Box<CreatedFleet>),
    /// The derived name was taken, so nothing was written.
    DerivedNameTaken,
}

/// The fields of one create every step reads.
struct Create<'a> {
    access: Access<'a>,
    request: &'a CreateFleetRequest,
    operation: Uuid7,
    digest: String,
    now: Timestamp,
}

impl Creations {
    /// Creates an active fleet, its thread and its first turn from `request`.
    ///
    /// # Errors
    /// `UZ-SETUP-005` for a key another body or person used, `UZ-SETUP-013`
    /// for a key whose fleet was deleted, `UZ-SETUP-008` when no model
    /// resolves, `UZ-AGT-006` for a name the workspace holds, an invalid
    /// name, a caller who is not a current member, a spent turn ceiling, and
    /// datastore failures. Every refusal writes nothing.
    pub async fn create(
        &self,
        access: Access<'_>,
        request: &garde::Valid<CreateFleetRequest>,
        now: Timestamp,
    ) -> Result<CreatedFleet> {
        if let Some(name) = &request.name {
            FleetName::parse(name)?;
        }
        let digest = if request.images.is_empty() {
            replay::digest(&(&request.message, &request.model_ref, &request.name))?
        } else {
            replay::digest(&(
                &request.message,
                &request.model_ref,
                &request.name,
                &request.images,
            ))?
        };
        let create = Create {
            access,
            request,
            operation: Uuid7::parse(&request.operation_id)?,
            digest,
            now,
        };
        // A derived name meets a taken one only when the random tail of two
        // identifiers collides, so a second try with a fresh one settles it.
        match self.attempt(&create).await? {
            Attempt::Done(created) => Ok(*created),
            Attempt::DerivedNameTaken => match self.attempt(&create).await? {
                Attempt::Done(created) => Ok(*created),
                Attempt::DerivedNameTaken => Err(ErrorKind::NameExists.into()),
            },
        }
    }

    async fn attempt(&self, create: &Create<'_>) -> Result<Attempt> {
        let id = self.mint(create.now)?;
        let name = create
            .request
            .name
            .clone()
            .unwrap_or_else(|| derived_name(&id));
        let key = turn_key(&id, &create.operation);
        let payload = if create.request.images.is_empty() {
            serde_json::json!({ "message": create.request.message })
        } else {
            serde_json::json!({ "message": create.request.message, "images": create.request.images })
        }.to_string();
        let actor = afd_turns::view::steer_actor(create.access.subject);
        // Read before the transaction opens: the ledger's ceiling sample may
        // take a connection of its own.
        let prepared = self
            .turns
            .prepare(IncomingTurn {
                producer: Producer::Steer,
                key: Key::Repeated(&key),
                fleet: &id,
                workspace: create.access.workspace,
                actor: &actor,
                event_type: TurnKind::Chat,
                request_json: &payload,
                reply: Reply::None,
            })
            .await?;
        let mut connection = self.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, create.access).await?;
        let scope = format!("{}:fleet:create:{}", create.access.workspace, person.id);
        if let Some(existing) = self.receipt(&mut tx, create, &scope, &person).await? {
            tx.commit().await.map_err(query(CONTEXT))?;
            // The replay reads on connections of its own; holding this one while
            // it waits for them starves a pool every racing caller holds a slot of.
            drop(connection);
            return self
                .replayed(create, &existing)
                .await
                .map(|created| Attempt::Done(Box::new(created)));
        }
        let model = model::resolve(
            &mut tx,
            &person.owner,
            create.request.model_ref.as_deref(),
            !create.request.images.is_empty(),
        )
        .await?;
        let reserved = replay::reserve(
            &mut tx,
            replay::Reservation {
                person: &person,
                scope: &scope,
                operation: &create.operation,
                digest: &create.digest,
                resource: &id,
                window: self.tuning.replay_window,
            },
        )
        .await?;
        if !reserved {
            // Another call with this key committed first: answer its fleet.
            let existing = self
                .receipt(&mut tx, create, &scope, &person)
                .await?
                .ok_or(ErrorKind::Replay)?;
            tx.commit().await.map_err(query(CONTEXT))?;
            drop(connection);
            return self
                .replayed(create, &existing)
                .await
                .map(|created| Attempt::Done(Box::new(created)));
        }
        let inserted = insert(
            &mut tx,
            &id,
            create.access.workspace,
            &person,
            &name,
            &model,
        )
        .await?;
        if !inserted {
            return match create.request.name {
                Some(_) => Err(ErrorKind::NameExists.into()),
                None => Ok(Attempt::DerivedNameTaken),
            };
        }
        let admitted = self.turns.stage(&mut tx, prepared).await?;
        let row = read::stored(&mut tx, create.access.workspace, &id).await?;
        tx.commit().await.map_err(query(CONTEXT))?;
        self.announce_created(create.access.workspace, &id).await;
        Ok(Attempt::Done(Box::new(CreatedFleet {
            fleet: read::project(row, Vec::new(), Vec::new())?,
            turn_id: admitted.stored.id,
            replayed: false,
        })))
    }

    /// The fleet an earlier call with this key created, when one did.
    async fn receipt(
        &self,
        connection: &mut PgConnection,
        create: &Create<'_>,
        scope: &str,
        person: &Person,
    ) -> Result<Option<Uuid7>> {
        replay::find(
            connection,
            scope,
            &create.operation,
            &person.id,
            &create.digest,
            self.tuning.replay_window,
        )
        .await
    }

    /// The answer to a repeat: the first call's fleet and turn, as they read now.
    async fn replayed(&self, create: &Create<'_>, fleet: &Uuid7) -> Result<CreatedFleet> {
        let turn = {
            let mut connection = self.database.acquire().await?;
            first_turn(&mut connection, fleet, &turn_key(fleet, &create.operation)).await?
        };
        let fleet = self
            .read(create.access, fleet)
            .await
            .map_err(gone_when_missing)?;
        Ok(CreatedFleet {
            fleet,
            turn_id: turn,
            replayed: true,
        })
    }

    /// Rings the new fleet's wake and tells the workspace's open walls.
    async fn announce_created(&self, workspace: &Uuid7, fleet: &Uuid7) {
        let streams = self.turns.streams();
        afd_bus::wake::publish(&streams, fleet.clone()).await;
        afd_turns::view::activity::fleets_changed(&streams, workspace).await;
        let (event, fleet_id) = (EVENT_CREATED, fleet.as_str());
        tracing::info!(event, fleet_id);
    }
}

/// The name a fleet gets when the person names none.
fn derived_name(id: &Uuid7) -> String {
    let digits: String = id
        .as_str()
        .chars()
        .filter(char::is_ascii_hexdigit)
        .collect();
    let tail = digits
        .get(digits.len().saturating_sub(DERIVED_NAME_DIGITS)..)
        .unwrap_or(&digits);
    format!("{DERIVED_NAME_PREFIX}{tail}")
}

/// The first turn's ledger key: the fleet and the create's operation, as a
/// steer with that operation would be keyed on that fleet.
fn turn_key(fleet: &Uuid7, operation: &Uuid7) -> String {
    format!("{fleet}:{operation}")
}

/// A fleet a receipt named, deleted after the receipt was read.
fn gone_when_missing(error: crate::Error) -> crate::Error {
    if matches!(error.kind(), ErrorKind::FleetMissing) {
        ErrorKind::Gone.into()
    } else {
        error
    }
}

/// Writes the fleet and its thread; `false` when the workspace holds `name`.
async fn insert(
    connection: &mut PgConnection,
    id: &Uuid7,
    workspace: &Uuid7,
    person: &Person,
    name: &str,
    model: &str,
) -> Result<bool> {
    let thread: Option<Uuid7> = sqlx::query_scalar(sql::INSERT)
        .bind(id)
        .bind(workspace)
        .bind(&person.owner)
        .bind(name)
        .bind(ACTIVE)
        .bind(model)
        .bind(af_fleet_runtime::config::HttpEgress::Restricted.as_str())
        .bind(false)
        .bind(INITIAL_VERSION)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(thread.is_some())
}

async fn first_turn(connection: &mut PgConnection, fleet: &Uuid7, key: &str) -> Result<Uuid7> {
    let turn: Option<Uuid7> = sqlx::query_scalar(sql::FIRST_TURN)
        .bind(fleet)
        .bind(Producer::Steer.as_str())
        .bind(key)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    // The turn committed with the fleet, so a missing one is a stored fault.
    turn.ok_or_else(|| absent("producer_key"))
}

#[cfg(test)]
mod tests {
    use af_core::id::Uuid7;

    use super::{DERIVED_NAME_DIGITS, DERIVED_NAME_PREFIX, derived_name, turn_key};

    #[expect(clippy::expect_used, reason = "fixture identifiers are well formed")]
    fn id(text: &str) -> Uuid7 {
        Uuid7::parse(text).expect("a canonical identifier")
    }

    #[test]
    fn a_derived_name_is_the_ids_last_twelve_hex_digits() {
        let fleet = id("019a0000-0000-7000-8000-00c0ffee1234");

        let name = derived_name(&fleet);

        assert_eq!(name, "fleet-00c0ffee1234");
        assert_eq!(name.len(), DERIVED_NAME_PREFIX.len() + DERIVED_NAME_DIGITS);
        assert!(af_fleet_runtime::FleetName::parse(&name).is_ok(), "{name}");
    }

    #[test]
    fn two_ids_differing_only_in_their_tail_derive_two_names() {
        let first = derived_name(&id("019a0000-0000-7000-8000-000000000001"));
        let second = derived_name(&id("019a0000-0000-7000-8000-000000000002"));

        assert_ne!(first, second);
    }

    #[test]
    fn the_first_turns_key_is_the_fleet_and_the_operation() {
        let fleet = id("019a0000-0000-7000-8000-000000000001");
        let operation = id("019a0000-0000-7000-8000-000000000002");

        assert_eq!(
            turn_key(&fleet, &operation),
            "019a0000-0000-7000-8000-000000000001:019a0000-0000-7000-8000-000000000002"
        );
    }

    #[test]
    fn a_receipts_fleet_deleted_before_its_read_answers_gone() {
        let missing = super::gone_when_missing(super::ErrorKind::FleetMissing.into());
        let other = super::gone_when_missing(super::ErrorKind::Replay.into());

        assert_eq!(missing.code(), af_core::error_code::SETUP_GONE);
        assert!(
            matches!(other.kind(), super::ErrorKind::Replay),
            "{other:?}"
        );
    }
}
