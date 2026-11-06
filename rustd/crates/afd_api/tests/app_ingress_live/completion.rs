//! A matching native task proves the signed route retains early completion evidence.

use super::Fixture;
use afd_db::test_util::mint_id;
use afd_wire::decision::{DecisionQuestion, NamedQuestion, NoulCriteria};
use sqlx::Acquire as _;

impl Fixture {
    pub(in super::super) async fn seed_completion_owner(&self) {
        let task = mint_id();
        let item = mint_id();
        let mut connection = self.database.acquire().await.expect("API connection");
        let mut tx = connection.begin().await.expect("task fixture transaction");
        sqlx::query(include_str!(
            "../integration_fleet_tasks/fixtures/seed_item.sql"
        ))
        .bind(&item)
        .bind(self.fleet.as_str())
        .bind(self.workspace.as_str())
        .bind(mint_id())
        .bind(0_i32)
        .bind(&task)
        .execute(&mut *tx)
        .await
        .expect("immutable task input");
        sqlx::query(include_str!(
            "../integration_fleet_tasks/fixtures/seed_task.sql"
        ))
        .bind(&task)
        .bind(self.fleet.as_str())
        .bind(&item)
        .bind("Deployment repair")
        .bind(
            serde_json::to_string(&[NamedQuestion {
                name: "deployment_restored".into(),
                question: DecisionQuestion::Noul {
                    instructions: "Is the deployment repaired?".into(),
                    criteria: NoulCriteria {
                        yes: "Deployment passes.".into(),
                        no: "Deployment fails.".into(),
                    },
                },
            }])
            .expect("typed questions serialize"),
        )
        .bind(afd_wire::task::TaskState::Waiting.as_str())
        .bind(false)
        .bind(0_i64)
        .execute(&mut *tx)
        .await
        .expect("unfinished native task");
        sqlx::query("UPDATE fleet.tasks SET target = $2 WHERE id = $1::uuid")
            .bind(&task)
            .bind(sqlx::types::Json(serde_json::json!({
                "repository": "example/platform",
                "installation_id": self.installation.parse::<u64>().expect("numeric installation"),
                "repository_id": 65_432_109,
                "number": 7,
                "branch": "repair/deployment",
                "base": "main",
                "head": "f1d2c3b4a5968778695a4b3c2d1e0f9a8b7c6d5e",
                "tree": "a1b2c3d4e5f60718293a4b5c6d7e8f9a0b1c2d3e",
                "url": "https://github.com/example/platform/pull/7",
                "workflows": [{"id": 87_654_321, "path": ".github/workflows/deploy.yml",
                    "digest": "a".repeat(64)}]
            })))
            .execute(&mut *tx)
            .await
            .expect("exact signed completion target");
        tx.commit().await.expect("task fixture commits");
    }

    pub(in super::super) async fn completion_receipts(&self, body: &str) -> i64 {
        let mut connection = self.database.acquire().await.expect("API connection");
        sqlx::query_scalar(
            "SELECT count(*) FROM fleet.task_completions c \
             JOIN fleet.task_completion_fanout f ON f.task_completion_id = c.id \
             WHERE c.workspace_id = $1::uuid AND c.scope_fleet_id IS NULL \
               AND c.installation_id = $2 AND c.body_digest = $3 \
               AND c.repository_id = 65_432_109 AND c.workflow_id = 87_654_321 \
               AND c.run_id = 9842731401 AND c.run_number = 412 \
               AND c.run_attempt = 2 AND c.conclusion = 'failure' \
               AND c.head_oid = 'f1d2c3b4a5968778695a4b3c2d1e0f9a8b7c6d5e'",
        )
        .bind(self.workspace.as_str())
        .bind(
            self.installation
                .parse::<i64>()
                .expect("numeric installation"),
        )
        .bind(afd_ingress::replay_id(body.as_bytes()))
        .fetch_one(&mut *connection)
        .await
        .expect("exact completion and its pending fanout read")
    }
}
