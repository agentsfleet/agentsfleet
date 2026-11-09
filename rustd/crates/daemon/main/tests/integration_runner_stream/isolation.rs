//! A private migrated database keeps absence assertions and statement counters scoped to one daemon.
#![expect(clippy::expect_used, reason = "integration preconditions fail loudly")]

use crate::{
    e2e::{Scenario, scenario_with},
    e2e_seed,
};
use afd_db::{Db, Migrator, config::DbRole, test_util::TestDatabase};
use agentsfleetd::supervisor::Supervisor;

pub(super) async fn open(
    supervisor: &mut Supervisor,
    extra: &[(&str, &str)],
) -> (TestDatabase, Db, Scenario) {
    let database = TestDatabase::create().await;
    let owner = database.open(DbRole::Migrator, &[]).await;
    Migrator::new()
        .run(&owner)
        .await
        .expect("the real schema migrates");
    sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_stat_statements")
        .execute(&mut *owner.acquire().await.expect("owner connection"))
        .await
        .expect("real statement counters are available");
    let api = database.api_url();
    let mut settings = vec![("DATABASE_URL_API", api.as_str())];
    settings.extend_from_slice(extra);
    let run = scenario_with(supervisor, None, &settings, e2e_seed::FLEET_CONFIG_JSON).await;
    (database, owner, run)
}
