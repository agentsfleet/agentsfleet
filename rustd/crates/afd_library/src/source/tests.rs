#![expect(clippy::expect_used, reason = "tests inspect fixtures directly")]

use std::sync::{Arc, Mutex};

use object_store::ObjectStore;
use object_store::memory::InMemory;

use super::{BundleSource, SourceFailure, SourceImporter};
use crate::error::ErrorKind;
use crate::{
    BundleCatalog, ImportBody, ImportService, PreparedBundle, Result, SourceKind, SupportFile,
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
            Self::Failure(failure) => Err((*failure).into()),
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
    ) -> impl std::future::Future<Output = Result<String>> + Send {
        let id = bundle.name.clone();
        self.0
            .lock()
            .expect("catalog mutex is healthy")
            .push(bundle.clone());
        std::future::ready(Ok(id))
    }
}

fn fixture() -> ImportBody {
    ImportBody {
        source_kind: SourceKind::Github,
        source_ref: "agentsfleet/reviewer".into(),
        source_revision: Some("main".into()),
        skill_markdown: b"---\nname: reviewer\ndescription: Reviews code\nversion: 1.0.0\n---\nInstructions.\n".to_vec(),
        trigger_markdown: Some(b"---\nname: reviewer\nx-agentsfleet:\n  triggers:\n    - type: cron\n      schedule: '0 * * * *'\n  credentials: [github]\n  tools: [http_request]\n  network:\n    allow: [api.github.com]\n  budget:\n    daily_dollars: 1\n---\n".to_vec()),
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

    assert_eq!(imported.bundle.name, "reviewer");
    assert_eq!(imported.bundle.description, "Reviews code");
    assert_eq!(imported.bundle.requirements.credentials, ["github"]);
    assert_eq!(imported.bundle.requirements.tools, ["http_request"]);
    assert_eq!(
        imported.bundle.requirements.network_hosts,
        ["api.github.com"]
    );
    assert_eq!(
        imported.bundle.requirements.support_files,
        ["docs/guide.md"]
    );
    assert_eq!(
        catalog
            .0
            .lock()
            .expect("catalog mutex is healthy")
            .as_slice(),
        [imported.bundle]
    );
}

#[tokio::test]
async fn source_failure_never_persists_partial_catalogue_state() {
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

        assert!(matches!(error.kind(), ErrorKind::Source(actual) if *actual == failure));
        assert!(
            catalog
                .0
                .lock()
                .expect("catalog mutex is healthy")
                .is_empty()
        );
    }
}

/// A refused import's two failure lines say why without repeating the code,
/// which each line's own `error_code` field already carries, or a backtrace.
#[tokio::test]
async fn a_failed_source_and_bundle_import_reason_carries_no_code_prefix() {
    let capture = afd_core::test_util::trace::Capture::install();
    let mut unsafe_path = fixture();
    unsafe_path.support_files = vec![SupportFile {
        path: "../outside".into(),
        content: Vec::new(),
    }];
    let error = importer(FixtureSource::Bundle(unsafe_path), Catalog::default())
        .import("agentsfleet/reviewer")
        .await
        .expect_err("a traversing path is refused");

    for event in ["bundle_import_failed", "library_source_import_failed"] {
        let logged = capture.only(event);
        let reason = logged.field("reason").expect("the line says why");
        assert!(
            !reason.is_empty() && !reason.contains("[UZ-") && !reason.contains('\n'),
            "{event}: {reason}"
        );
        assert_eq!(
            logged.field("error_code"),
            Some(error.code().as_str()),
            "{event}"
        );
    }
}
