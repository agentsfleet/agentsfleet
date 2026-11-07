//! Real API-role catalogue pages never mix generations or lose a tied provider.
use super::{Lane, NOW, details};
use af_core::error_code;
use afd_admin::CreateModel;

async fn seed(lane: &Lane) {
    for suffix in ["a", "b", "c"] {
        let mut input = lane.input();
        input.model_name = suffix.to_owned();
        assert!(matches!(
            lane.models.create(input, NOW).await.expect("file"),
            CreateModel::Created(_)
        ));
    }
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn pages_replay_exactly_and_cover_the_complete_ordered_catalogue() {
    super::isolated::run(async |lane| {
        seed(lane).await;
        let all = lane.models.list().await.expect("all");
        let first = lane.models.page(1, None).await.expect("first");
        assert_eq!(first.models, all.get(..1).expect("first model"));
        let token = first.next_cursor.expect("continuation");
        let second = lane.models.page(1, Some(&token)).await.expect("second");
        let replay = lane.models.page(1, Some(&token)).await.expect("replay");
        assert_eq!(second.models, replay.models);
        assert_eq!(second.next_cursor, replay.next_cursor);
        assert_eq!(second.models, all.get(1..2).expect("second model"));
        let third = lane
            .models
            .page(1, second.next_cursor.as_deref())
            .await
            .expect("third");
        assert_eq!(third.models, all.get(2..).expect("remaining models"));
        assert!(third.next_cursor.is_none());
        assert!(
            lane.models
                .page(100, None)
                .await
                .expect("large page")
                .next_cursor
                .is_none()
        );
    })
    .await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn page_bounds_tokens_and_changed_prices_are_refused() {
    super::isolated::run(async |lane| {
        seed(lane).await;
        invalid_pages(lane).await;
        let first = lane.models.page(1, None).await.expect("first");
        let token = first.next_cursor.expect("next");
        assert_eq!(
            lane.models
                .page(2, Some(&token))
                .await
                .expect_err("changed limit")
                .code(),
            error_code::INVALID_REQUEST
        );
        let model = first.models.first().expect("model");
        lane.models
            .update(&model.id, details(true), NOW)
            .await
            .expect("price replacement");
        assert_eq!(
            lane.models
                .page(1, Some(&token))
                .await
                .expect_err("stale")
                .code(),
            error_code::INVALID_REQUEST
        );
        assert_eq!(
            lane.models
                .page(1, None)
                .await
                .expect("restart")
                .models
                .first()
                .expect("restarted first model")
                .details,
            details(true)
        );
    })
    .await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_empty_catalogue_has_no_continuation() {
    super::isolated::run(async |lane| {
        let empty = lane.models.page(1, None).await.expect("empty");
        assert_eq!(empty.models.len(), 0);
        assert!(empty.next_cursor.is_none());
    })
    .await;
}

async fn invalid_pages(lane: &Lane) {
    for limit in [0, 101, u32::MAX] {
        assert_eq!(
            lane.models
                .page(limit, None)
                .await
                .expect_err("bound")
                .code(),
            error_code::INVALID_REQUEST
        );
    }
    for token in ["", "not-a-cursor", "e30"] {
        assert_eq!(
            lane.models
                .page(1, Some(token))
                .await
                .expect_err("token")
                .code(),
            error_code::INVALID_REQUEST
        );
    }
}
