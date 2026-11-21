//! What a held run does over time: an assignment superseding its fence, and
//! the meter's slices.

use jiff::Timestamp;

use super::{Held, Row, SLICE};

impl Held {
    /// Moves the fleet's live sequence one past the held run, as a won assignment
    /// does before it expires the prior run, and answers the new sequence.
    pub(crate) async fn supersede(&self) -> u64 {
        let mut connection = self
            .fixtures
            .database
            .acquire()
            .await
            .expect("a pooled connection");
        let live: i64 = sqlx::query_scalar(
            "UPDATE thread.threads SET fencing_token = fencing_token + 1
             WHERE fleet_id = $1 RETURNING fencing_token",
        )
        .bind(&self.fleet)
        .fetch_one(&mut *connection)
        .await
        .expect("the held fleet has an affinity row");
        assert!(
            live > self.fence.as_i64(),
            "the sequence must outrank the held run, or this test proves nothing"
        );
        u64::try_from(live).expect("a sequence is never negative")
    }

    /// Ages the run's meter by one [`SLICE`], as a second of runtime would,
    /// and answers the stamp the next settle or renewal measures from.
    ///
    /// Settles and renewals meter up to the database's clock, so a test runs a
    /// slice by moving the meter's start back rather than by naming an instant.
    pub(crate) async fn run_a_slice(&self) -> Timestamp {
        // The run began a slice earlier too: its cursor never precedes its
        // start, because a runtime line is numbered by the milliseconds between
        // them.
        self.fixtures
            .age(Row::Run(&self.issued.run_id), "created_at", SLICE)
            .await;
        self.fixtures
            .age(
                Row::Run(&self.issued.run_id),
                crate::run_reads::COLUMN_LAST_METERED,
                SLICE,
            )
            .await;
        self.fixtures.metered_since(&self.fleet).await
    }

    /// The wall time between `from` and the meter's stamp now, which is the
    /// slice the last settle or renewal charged for.
    pub(crate) async fn metered_from(&self, from: Timestamp) -> i64 {
        let to = self.fixtures.metered_since(&self.fleet).await;
        af_core::clock::saturating_millis_signed(af_core::clock::elapsed(from, to))
    }
}
