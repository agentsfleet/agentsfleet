use super::{ACQUIRE_TIMEOUT_MS_DEFAULT, min_connections_default, pool_size_default};

/// The acquire budget covers opening a connection, not only waiting for one.
///
/// sqlx spends ONE deadline on both halves — `PoolInner::acquire` hands its
/// `acquire_timeout` deadline straight to `connect()` — so a budget under
/// the handshake cost is a pool that can never grow. It does not degrade,
/// it stops: `warm` reaches zero, the pool stays empty, and every request
/// pays and loses the same race while the census reads "below the
/// ceiling", which is indistinguishable from an outage.
///
/// The floor asserted here is a MEASURED handshake, not a guess: a probe
/// against the development database through the production pool took
/// 2.7 s to complete one, and at the previous 2 s budget warmed 0 of 2 and
/// failed all eight callers. Anything at or below that number reproduces
/// the failure exactly, so the test names it.
#[test]
fn test_the_acquire_budget_can_outlast_a_connection_handshake() {
    /// The slowest establishment measured against a real deployment, in
    /// milliseconds. A budget at or below this cannot open a connection.
    const MEASURED_SLOW_HANDSHAKE_MS: u64 = 2_700;

    // Through `black_box` so the comparison survives as a comparison:
    // both sides are constants, and clippy rejects an assertion it can
    // fold away — correctly, since a folded assert proves nothing at run
    // time. The budget is read the way a caller reads it instead.
    let budget = std::hint::black_box(ACQUIRE_TIMEOUT_MS_DEFAULT);
    let handshake = std::hint::black_box(MEASURED_SLOW_HANDSHAKE_MS);

    assert!(
        budget > handshake,
        "an acquire budget of {budget}ms cannot open a connection that takes {handshake}ms: \
         the pool would warm to zero and answer every request from empty"
    );
}

/// The ceiling is a real number, not four.
///
/// Four was `256 / 64`, and the 64 was a "per-connection request-sharing
/// factor" that nothing ever derived. What four produced was a pool that
/// spends its life below
/// its own ceiling, which is the exact state `Db::acquire` reads as an
/// absent datastore — so an undersized pool answered 503 while Postgres was
/// healthy. This asserts the floor that failure sat under.
#[test]
fn test_the_pool_ceiling_is_not_the_undersized_default_it_replaced() {
    const THE_OLD_DEFAULT: u32 = 256 / 64;

    let ceiling = pool_size_default();

    assert!(
        ceiling > THE_OLD_DEFAULT,
        "the ceiling must exceed the four connections that produced 503s \
         under an ordinary burst: {ceiling}"
    );
    assert!(
        ceiling <= 100,
        "and must stay inside a stock Postgres connection ceiling, which is \
         what `core_api` sizes against: {ceiling}"
    );
}

/// Some of the pool is warm before any request arrives.
///
/// This is the clause that removes a connection handshake from a request's
/// path. A floor of zero is what `connect_lazy_with` gives on its own, and
/// it is what made a cold pool's first traffic queue behind establishment
/// — measured at 147 ms typically and 337 ms under load — inside a budget
/// meant only for waiting.
#[test]
fn test_some_of_the_pool_is_established_before_it_is_asked_for() {
    let ceiling = pool_size_default();
    let warm = min_connections_default(ceiling);

    assert!(warm >= 1, "a pool that starts empty opens in the hot path");
    assert!(
        warm <= ceiling,
        "a floor above the ceiling is a pool that cannot satisfy its own \
         maintenance: {warm} > {ceiling}"
    );
}
