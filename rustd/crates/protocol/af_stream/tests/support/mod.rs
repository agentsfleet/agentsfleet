pub(crate) mod proxy;

use std::{
    convert::Infallible,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use af_stream::{
    Endpoint, Incoming, Outgoing,
    server::Listener,
    tls::{Identity, Trust},
};
use http::{Request, Response};
use tokio::time::Instant;
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};

pub(super) type Result<T = ()> = af_stream::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub(super) struct Fixture {
    _directory: tempfile::TempDir,
    pub(super) identity: Identity,
    pub(super) trust: Trust,
}

impl Fixture {
    pub(super) fn new(names: Vec<String>, expired: bool) -> Result<Self> {
        let directory = tempfile::tempdir()?;
        let mut root = rcgen::CertificateParams::new(Vec::<String>::new())?;
        root.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        root.distinguished_name
            .push(rcgen::DnType::CommonName, "runner test issuer");
        root.key_usages = vec![rcgen::KeyUsagePurpose::KeyCertSign];
        let root = rcgen::CertifiedIssuer::self_signed(root, rcgen::KeyPair::generate()?)?;
        let key = rcgen::KeyPair::generate()?;
        let mut parameters = rcgen::CertificateParams::new(names)?;
        if expired {
            parameters.not_before = rcgen::date_time_ymd(2010, 1, 1);
            parameters.not_after = rcgen::date_time_ymd(2011, 1, 1);
        }
        let certificate = parameters.signed_by(&key, &root)?;
        let chain = directory.path().join("chain.pem");
        let private = directory.path().join("key.pem");
        let roots = directory.path().join("roots.pem");
        std::fs::write(&chain, format!("{}{}", certificate.pem(), root.pem()))?;
        std::fs::write(&private, key.serialize_pem())?;
        std::fs::write(&roots, root.pem())?;
        Ok(Self {
            _directory: directory,
            identity: Identity::new(chain, private),
            trust: Trust::with_roots(roots),
        })
    }

    pub(super) async fn serve(&self) -> Result<Host> {
        self.serve_mode(false).await
    }

    pub(super) async fn serve_without_headers(&self) -> Result<Host> {
        self.serve_mode(true).await
    }

    async fn serve_mode(&self, hold_headers: bool) -> Result<Host> {
        let listener = Listener::bind("[::1]:0".parse()?, self.identity.clone()).await?;
        let endpoint = Endpoint::parse(&format!(
            "https://localhost:{}",
            listener.local_addr()?.port()
        ))?;
        let stop = CancellationToken::new();
        let connections = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&connections);
        let served = Arc::clone(&requests);
        let task = tokio::spawn(listener.serve(
            move |_connection| {
                counted.fetch_add(1, Ordering::SeqCst);
                let served = Arc::clone(&served);
                tower::service_fn(move |request: Request<Incoming>| {
                    served.fetch_add(1, Ordering::SeqCst);
                    async move {
                        if hold_headers {
                            std::future::pending::<()>().await;
                        }
                        Ok::<_, Infallible>(Response::new(Outgoing::new(request.into_body())))
                    }
                })
            },
            stop.clone(),
            stop.clone(),
        ));
        Ok(Host {
            endpoint,
            stop,
            connections,
            requests,
            task: AbortOnDropHandle::new(task),
        })
    }
}

pub(super) struct Host {
    pub(super) endpoint: Endpoint,
    pub(super) stop: CancellationToken,
    pub(super) connections: Arc<AtomicUsize>,
    pub(super) requests: Arc<AtomicUsize>,
    task: AbortOnDropHandle<af_stream::Result<()>>,
}

impl Host {
    pub(super) async fn close(mut self) -> Result {
        self.stop.cancel();
        tokio::time::timeout(Duration::from_secs(5), &mut self.task).await???;
        Ok(())
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

pub(super) fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}
