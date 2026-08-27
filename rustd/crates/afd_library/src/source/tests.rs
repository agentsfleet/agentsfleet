#![expect(clippy::expect_used, reason = "tests inspect fixtures directly")]

use std::sync::{Arc, Mutex};

use object_store::ObjectStore;
use object_store::memory::InMemory;

use super::{BundleSource, SourceFailure, SourceImporter};
use crate::{
    BundleCatalog, Error, ImportBody, ImportService, PreparedBundle, Result, SourceKind,
    SupportFile,
};

#[derive(Debug, Clone)]
enum FixtureSource {
    Bundle(ImportBody),
    Failure(SourceFailure),
}

impl BundleSource for FixtureSource {
    fn fetch(
        &self,
        _reference: &str,
    ) -> impl std::future::Future<Output = Result<ImportBody>> + Send {
        std::future::ready(match self {
            Self::Bundle(body) => Ok(body.clone()),
            Self::Failure(failure) => Err(Error::Source(*failure)),
        })
    }
}

#[derive(Debug, Clone, Default)]
struct Catalog(Arc<Mutex<Vec<PreparedBundle>>>);

impl BundleCatalog for Catalog {
    fn insert(
        &self,
        _body: &ImportBody,
        bundle: &PreparedBundle,
    ) -> impl std::future::Future<Output = Result<()>> + Send {
        self.0
            .lock()
            .expect("catalog mutex is healthy")
            .push(bundle.clone());
        std::future::ready(Ok(()))
    }
}

fn fixture() -> ImportBody {
    ImportBody {
        source_kind: SourceKind::Github,
        source_ref: "agentsfleet/reviewer".into(),
        source_revision: Some("main".into()),
        skill_markdown: b"---\nname: reviewer\ndescription: Reviews code\nversion: 1.0.0\n---\nInstructions.\n".to_vec(),
        trigger_markdown: Some(b"---\nname: reviewer\nx-agentsfleet:\n  credentials: [github]\n  tools: [http_request]\n  network:\n    allow: [api.github.com]\n---\n".to_vec()),
        support_files: vec![SupportFile { path: "docs/guide.md".into(), content: b"guide".to_vec() }],
    }
}

fn importer(source: FixtureSource, catalog: Catalog) -> SourceImporter<FixtureSource, Catalog> {
    let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
    SourceImporter::new(source, ImportService::new(store, catalog))
}

#[tokio::test]
async fn test_library_import_parity() {
    let catalog = Catalog::default();
    let source = FixtureSource::Bundle(fixture());
    let imported = importer(source, catalog.clone())
        .import("agentsfleet/reviewer")
        .await
        .expect("fixture import succeeds");

    assert_eq!(imported.name, "reviewer");
    assert_eq!(imported.description, "Reviews code");
    assert_eq!(imported.requirements.credentials, ["github"]);
    assert_eq!(imported.requirements.tools, ["http_request"]);
    assert_eq!(imported.requirements.network_hosts, ["api.github.com"]);
    assert_eq!(imported.requirements.support_files, ["docs/guide.md"]);
    assert_eq!(
        catalog
            .0
            .lock()
            .expect("catalog mutex is healthy")
            .as_slice(),
        [imported]
    );
}

#[tokio::test]
async fn test_library_import_failure_classes() {
    for failure in [
        SourceFailure::NotFound,
        SourceFailure::RateLimited,
        SourceFailure::Truncated,
    ] {
        let catalog = Catalog::default();
        let error = importer(FixtureSource::Failure(failure), catalog.clone())
            .import("agentsfleet/missing")
            .await
            .expect_err("source failure is retained");

        assert!(matches!(error, Error::Source(actual) if actual == failure));
        assert!(
            catalog
                .0
                .lock()
                .expect("catalog mutex is healthy")
                .is_empty()
        );
    }
}
