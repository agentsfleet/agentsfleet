#![expect(clippy::expect_used, reason = "tests inspect successful fixtures")]

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use afd_fleet::bundle::{Bundles, ContentHash};
use async_trait::async_trait;
use futures_util::stream::BoxStream;
use object_store::memory::InMemory;
use object_store::path::Path;
use object_store::{
    CopyOptions, GetOptions, GetResult, ListResult, MultipartUpload, ObjectMeta, ObjectStore,
    PutMultipartOptions, PutOptions, PutPayload, PutResult,
};

use super::{BundleCatalog, ImportService};
use crate::{ImportBody, PreparedBundle, SourceKind, SupportFile};

#[derive(Debug, Default)]
struct MemoryCatalog(Mutex<Vec<PreparedBundle>>);

impl BundleCatalog for MemoryCatalog {
    fn insert(
        &self,
        _body: &ImportBody,
        bundle: &PreparedBundle,
    ) -> impl std::future::Future<Output = crate::Result<()>> + Send {
        self.0
            .lock()
            .expect("catalog mutex is healthy")
            .push(bundle.clone());
        std::future::ready(Ok(()))
    }
}

#[derive(Debug, Default)]
struct FailFirstPut {
    failed: AtomicBool,
    inner: InMemory,
}

impl fmt::Display for FailFirstPut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("fail-first-put")
    }
}

#[async_trait]
impl ObjectStore for FailFirstPut {
    async fn put_opts(
        &self,
        location: &Path,
        payload: PutPayload,
        opts: PutOptions,
    ) -> object_store::Result<PutResult> {
        if !self.failed.swap(true, Ordering::SeqCst) {
            return Err(object_store::Error::Generic {
                store: "fixture",
                source: Box::new(std::io::Error::other("R2 unavailable")),
            });
        }
        self.inner.put_opts(location, payload, opts).await
    }

    async fn put_multipart_opts(
        &self,
        location: &Path,
        opts: PutMultipartOptions,
    ) -> object_store::Result<Box<dyn MultipartUpload>> {
        self.inner.put_multipart_opts(location, opts).await
    }

    async fn get_opts(
        &self,
        location: &Path,
        options: GetOptions,
    ) -> object_store::Result<GetResult> {
        self.inner.get_opts(location, options).await
    }

    fn delete_stream(
        &self,
        locations: BoxStream<'static, object_store::Result<Path>>,
    ) -> BoxStream<'static, object_store::Result<Path>> {
        self.inner.delete_stream(locations)
    }

    fn list(&self, prefix: Option<&Path>) -> BoxStream<'static, object_store::Result<ObjectMeta>> {
        self.inner.list(prefix)
    }

    fn list_with_offset(
        &self,
        prefix: Option<&Path>,
        offset: &Path,
    ) -> BoxStream<'static, object_store::Result<ObjectMeta>> {
        self.inner.list_with_offset(prefix, offset)
    }

    async fn list_with_delimiter(&self, prefix: Option<&Path>) -> object_store::Result<ListResult> {
        self.inner.list_with_delimiter(prefix).await
    }

    async fn copy_opts(
        &self,
        from: &Path,
        to: &Path,
        options: CopyOptions,
    ) -> object_store::Result<()> {
        self.inner.copy_opts(from, to, options).await
    }
}

fn bundle() -> ImportBody {
    ImportBody {
        source_kind: SourceKind::Upload,
        source_ref: "unit".into(),
        source_revision: None,
        skill_markdown:
            b"---\nname: reviewer\ndescription: Reviews code\nversion: 1.0.0\n---\nInstructions.\n"
                .to_vec(),
        trigger_markdown: None,
        support_files: vec![SupportFile {
            path: "docs/guide.md".into(),
            content: b"guide".to_vec(),
        }],
    }
}

#[tokio::test]
async fn test_bundle_import_roundtrip() {
    let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
    let serving = Bundles::new(Arc::clone(&store));
    let importer = ImportService::new(store, MemoryCatalog::default());

    let prepared = importer
        .import(&bundle())
        .await
        .expect("the import succeeds");
    let hash = ContentHash::parse(&prepared.content_hash).expect("the importer emits a digest");
    let served = serving
        .fetch(hash)
        .await
        .expect("the M177 service finds the snapshot");

    assert_eq!(
        served,
        importer
            .snapshot(&prepared.snapshot_key)
            .await
            .expect("the writer reads identical bytes")
    );
    assert_eq!(prepared.requirements.support_files, ["docs/guide.md"]);
    assert_eq!(
        importer
            .catalog
            .0
            .lock()
            .expect("catalog mutex is healthy")
            .len(),
        1
    );
}

#[tokio::test]
async fn test_bundle_import_r2_outage() {
    let importer = ImportService::new(Arc::new(FailFirstPut::default()), MemoryCatalog::default());

    let error = importer
        .import(&bundle())
        .await
        .expect_err("the injected first write fails");

    assert!(error.retryable());
    assert_eq!(error.code().as_str(), "UZ-BUNDLE-005");
    assert!(
        importer
            .catalog
            .0
            .lock()
            .expect("catalog mutex is healthy")
            .is_empty()
    );

    importer
        .import(&bundle())
        .await
        .expect("retry succeeds after the transient outage");
    assert_eq!(
        importer
            .catalog
            .0
            .lock()
            .expect("catalog mutex is healthy")
            .len(),
        1
    );
}

#[tokio::test]
async fn skill_only_import_needs_no_snapshot_store() {
    let mut body = bundle();
    body.support_files.clear();
    let importer = ImportService::without_store(MemoryCatalog::default());

    importer
        .import(&body)
        .await
        .expect("skill-only metadata imports without R2");
    assert_eq!(
        importer
            .catalog
            .0
            .lock()
            .expect("catalog mutex is healthy")
            .len(),
        1
    );
}
