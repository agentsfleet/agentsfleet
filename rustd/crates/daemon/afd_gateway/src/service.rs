//! The native listener reuses the HTTP router and its chosen authentication strategy.

use std::{convert::Infallible, sync::Arc};

use af_stream::{ConnectionInfo, Incoming, Outgoing};
use axum::{Router, body::Body};
use http::{Request, Response};
use tower::{Service, ServiceExt as _};

use crate::{ConnectionContext, Gateway};

impl Gateway {
    /// Construct once per physical connection, then share its context across request streams.
    pub fn connection_service(
        router: Router,
        info: ConnectionInfo,
    ) -> impl Service<
        Request<Incoming>,
        Response = Response<Outgoing>,
        Error = Infallible,
        Future: Send,
    > + Clone
    + Send {
        let context = Arc::new(ConnectionContext::new(info));
        tower::service_fn(move |request: Request<Incoming>| {
            let router = router.clone();
            let context = Arc::clone(&context);
            async move {
                if request.uri().path() != af_wire::paths::RUNNER_STREAM {
                    let mut response = Response::new(Outgoing::new(Body::empty()));
                    *response.status_mut() = http::StatusCode::NOT_FOUND;
                    return Ok::<_, Infallible>(response);
                }
                let mut request = request.map(Body::new);
                request
                    .extensions_mut()
                    .insert(afd_http::auth::NativeRunnerConnection::new(Arc::clone(
                        &context,
                    )));
                request.extensions_mut().insert(context);
                let response = match router.oneshot(request).await {
                    Ok(response) => response,
                    Err(never) => match never {},
                };
                Ok::<_, Infallible>(response.map(Outgoing::new))
            }
        })
    }
}
