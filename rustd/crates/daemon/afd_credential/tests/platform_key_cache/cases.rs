use super::*;

// Value: protects=a same-KEK key replace reaches the next lease; fails_when=the
// cache keys on the secret's id or the catalogue alone; why_new=a replace keeps
// the id and kek_version; seam=none; no_claim=a rotation during a lease.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn platform_key_cache_misses_on_same_kek_rotation() {
    let lane = Lane::create().await;
    lane.key("sk-key-a", false).await;
    lane.activate_default().await;

    let window = Window::open(&lane.owner).await;
    assert_eq!(lane.lease(PLATFORM_MODEL_REF).await, "sk-key-a");
    assert_eq!(
        lane.lease(PLATFORM_MODEL_REF).await,
        "sk-key-a",
        "the held version answers"
    );
    lane.key("sk-key-b", true).await;
    assert_eq!(
        lane.lease(PLATFORM_MODEL_REF).await,
        "sk-key-b",
        "the moved version misses"
    );
    let ran = window.close().await;

    assert_eq!(
        secret_reads(&ran),
        2,
        "read for lease one and after the rotation: {:?}",
        ran.statements()
    );
    lane.cleanup().await;
}

// Value: protects=tenant references stay uncached; fails_when=a tenant entry is
// served from the platform cache; why_new=the cache sits on the resolution
// path both kinds share; seam=none; no_claim=a tenant entry, which is never cached.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn tenant_model_reference_reads_every_lease() {
    let lane = Lane::create().await;
    lane.key("sk-tenant", false).await;
    lane.activate_default().await;
    let Added::Stored(entry) = lane
        .providers
        .add_entry(
            &lane.tenant,
            MODEL,
            KeyRef {
                workspace_id: &lane.workspace,
                name: PROVIDER,
            },
            NOW,
        )
        .await
        .expect("an entry over a stored credential is an outcome")
    else {
        panic!("the credential is stored, so the entry stores");
    };
    let reference = entry.id.as_str().to_owned();

    let window = Window::open(&lane.owner).await;
    for _ in 0..2 {
        assert_eq!(lane.lease(&reference).await, "sk-tenant");
    }
    let ran = window.close().await;

    let entry_reads: u64 = ran
        .statements()
        .iter()
        .filter(|(query, _)| {
            query.contains("FROM model.tenant_models e") && query.contains("resolution_version")
        })
        .map(|(_, calls)| calls)
        .sum();
    assert_eq!(entry_reads, 2, "{:?}", ran.statements());
    assert_eq!(secret_reads(&ran), 2, "{:?}", ran.statements());
    lane.cleanup().await;
}

// Value: protects=the witness moves with the stored bytes and the endpoint and
// with nothing else; fails_when=the digest ignores the ciphertext or the
// endpoint, or moves on a write that changed neither; why_new=the counter that
// stood here is gone and the cache now trusts the bytes themselves;
// seam=none; no_claim=a rotation between the claim and the read.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn secret_digest_follows_the_stored_bytes_and_endpoint() {
    let lane = Lane::create().await;
    lane.key("sk-key-a", false).await;
    lane.activate_default().await;
    let first = lane.observed().await.key;

    lane.touch().await;
    assert_eq!(
        lane.observed().await.key,
        first,
        "a write that changed neither the bytes nor the endpoint is no rotation"
    );

    lane.key("sk-key-a", true).await;
    let replaced = lane.observed().await.key;
    assert_ne!(
        replaced, first,
        "the same plaintext sealed again is another envelope, and a replace under \
         the same KEK keeps the row's id and kek_version"
    );

    lane.reseal().await;
    let resealed = lane.observed().await.key;
    assert_ne!(
        resealed, replaced,
        "a re-seal rewrites the whole envelope, so it moves the witness too: one \
         extra read, never a stale key"
    );

    lane.pin_endpoint(SAVED_BASE_URL).await;
    assert_ne!(
        lane.observed().await.key,
        resealed,
        "the endpoint beside the ciphertext is part of what the key answers for"
    );
    lane.cleanup().await;
}
