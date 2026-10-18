//! Real Postgres catalogue reads; provider evidence below is synthetic fixture data.

use afd_admin::{CreateModel, ModelInput, ModelUpdate};
use afd_core::catalogue::{Metadata, Price};
use afd_crypto::entropy::Entropy;
use afd_db::{Db, Migrator, config::DbRole, test_util::TestDatabase};
use afd_tenant::{
    Result,
    models::{Boundary, Models},
};
use jiff::Timestamp;
use std::error::Error as _;

const VERIFIED: Timestamp = Timestamp::constant(1_725_000_000, 0);
const PROVIDER: &str = "fireworks";
const FIRST: &str = "a-fixture";
const SECOND: &str = "b-fixture";
const TERRA: &str = "gpt-5.6-terra";
const EXPECTED_MODEL: &str = "the requested catalogue model must be present";
const EXPECTED_FAILURE: &str = "the unavailable catalogue must refuse the read";

struct Lane {
    database: TestDatabase,
    db: Db,
    models: Models,
    admin: afd_admin::Models,
}

impl Lane {
    async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let database = TestDatabase::create().await;
        let migrator = database.open(DbRole::Migrator, &[]).await;
        let migrated = Migrator::new().run(&migrator).await;
        migrator.close().await;
        if let Err(failure) = migrated {
            database.cleanup().await;
            return Err(failure.into());
        }
        let db = database.open(DbRole::Api, &[]).await;
        Ok(Self {
            models: Models::new(db.clone()),
            admin: afd_admin::Models::new(db.clone(), Entropy::new()),
            db,
            database,
        })
    }

    async fn seed(
        &self,
        model_name: &str,
        advanced: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let provider = if advanced { "openai" } else { PROVIDER };
        let input = ModelInput {
            provider: provider.to_owned(),
            model_name: model_name.to_owned(),
            details: ModelUpdate {
                metadata: Metadata {
                    display_name: format!("Readable {model_name}"),
                    context_cap_tokens: 1_048_576,
                    context_source_url: "https://example.test/models/fixture".to_owned(),
                    context_verified_at: VERIFIED,
                    advanced,
                },
                prices: vec![price(0), price(272_000)],
                inference: None,
            },
        };
        assert!(matches!(
            self.admin.create(input, VERIFIED).await?,
            CreateModel::Created(_)
        ));
        Ok(())
    }

    async fn cleanup(self) {
        self.db.close().await;
        self.database.cleanup().await;
    }
}

fn price(threshold: i32) -> Price {
    Price {
        effective_from: VERIFIED,
        effective_until: None,
        prompt_tokens_above: threshold,
        input_nanos_per_mtok: 37_500_000,
        cached_input_nanos_per_mtok: 6_000_000,
        cache_write_nanos_per_mtok: Some(187_500_000),
        cache_write_5m_nanos_per_mtok: Some(125_000_000),
        cache_write_1h_nanos_per_mtok: Some(200_000_000),
        output_nanos_per_mtok: 150_000_000,
        source_url: "https://example.test/pricing".to_owned(),
        verified_at: VERIFIED,
        conditions: "Synthetic price fixture; no inference request.".to_owned(),
    }
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_catalogue_walk_carries_metadata_prices_and_explicit_advanced_filter()
-> Result<(), Box<dyn std::error::Error>> {
    let lane = Lane::new().await?;
    lane.seed(FIRST, false).await?;
    lane.seed(SECOND, false).await?;
    lane.seed(TERRA, true).await?;
    let first = lane.models.page(None, false, None, 1).await?;
    assert!(first.has_more);
    assert_eq!(first.models.len(), 1);
    let row = first.models.first().ok_or(EXPECTED_MODEL)?;
    assert_eq!(row.model_id, FIRST);
    assert_eq!(row.metadata.display_name, format!("Readable {FIRST}"));
    assert_eq!(row.metadata.context_verified_at, VERIFIED);
    assert_eq!(row.prices, vec![price(0), price(272_000)]);
    let second = lane
        .models
        .page(None, false, first.boundary.as_ref(), 1)
        .await?;
    assert!(!second.has_more);
    assert_eq!(
        second.models.first().ok_or(EXPECTED_MODEL)?.model_id,
        SECOND
    );
    let regular = lane.models.page(None, false, None, 10).await?;
    assert_eq!(regular.models.len(), 2);
    let advanced = lane.models.page(Some("openai"), true, None, 10).await?;
    assert_eq!(advanced.models.len(), 1);
    let row = advanced.models.first().ok_or(EXPECTED_MODEL)?;
    assert_eq!(row.model_id, TERRA);
    assert!(row.metadata.advanced);
    assert_eq!(
        lane.models
            .page(Some("openai"), false, None, 10)
            .await?
            .models
            .len(),
        0
    );
    lane.cleanup().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_empty_catalogue_has_no_boundary_or_version() -> Result<(), Box<dyn std::error::Error>>
{
    let lane = Lane::new().await?;
    let page = lane.models.page(None, false, None, 10).await?;
    assert_eq!(page.models.len(), 0);
    assert!(page.boundary.is_none());
    assert!(page.max_updated.is_none());
    assert!(!page.has_more);
    lane.cleanup().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_catalogue_query_and_acquire_failures_keep_their_causes()
-> Result<(), Box<dyn std::error::Error>> {
    let lane = Lane::new().await?;
    let invalid = Boundary {
        display_key: String::new(),
        vendor_key: String::new(),
        id: "invalid-uuid".to_owned(),
    };
    let query = lane.models.page(None, false, Some(&invalid), 1).await;
    assert!(query.err().ok_or(EXPECTED_FAILURE)?.source().is_some());
    lane.db.close().await;
    let query = lane.models.page(None, false, None, 1).await;
    assert!(query.err().ok_or(EXPECTED_FAILURE)?.source().is_some());
    lane.cleanup().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_unreadable_context_evidence_refuses_the_page()
-> Result<(), Box<dyn std::error::Error>> {
    let lane = Lane::new().await?;
    lane.seed(FIRST, false).await?;
    let mut connection = lane.db.acquire().await?;
    sqlx::query("UPDATE model.platform_models SET context_verified_at = 'infinity'::timestamptz")
        .execute(&mut *connection)
        .await?;
    drop(connection);
    let query = lane.models.page(None, false, None, 1).await;
    assert!(query.err().ok_or(EXPECTED_FAILURE)?.source().is_some());
    lane.cleanup().await;
    Ok(())
}
