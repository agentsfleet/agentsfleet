//! The outbound HTTP clients boot opens, built before anything is spawned.
//!
//! Each is its own pool, because a pool is what a client is: the broker holds
//! its exchange client privately, so sharing it would let a slow connector
//! exchange take the slots a credential mint needs, and the delivery worker
//! runs beside the plane rather than through it. The three that carry a
//! vendor's credential are direct and bounded (`afd_identity::direct`); the
//! scheduler's is bounded and honours an environment proxy, as its default
//! client did. One that will not build refuses boot here, before a socket is
//! opened or a worker started, rather than standing in a default client.

use crate::error::BootFailure;

/// Every outbound client boot hands on, one pool per surface.
#[derive(Debug)]
pub(super) struct Clients {
    /// The credential broker's vendor token exchanges.
    pub(super) broker: reqwest::Client,
    /// The plane's connector token exchange and site listing.
    pub(super) plane: reqwest::Client,
    /// The answer-delivery worker's vendor calls.
    pub(super) outbound: reqwest::Client,
    /// The external scheduler's management calls.
    pub(super) schedule: reqwest::Client,
}

impl Clients {
    /// Builds every client this deployment sends through.
    ///
    /// # Errors
    /// [`BootFailure::HttpClient`] when any of them will not build.
    pub(super) fn open() -> Result<Self, BootFailure> {
        Self::open_with(
            crate::credentials::vendor_exchange_client,
            crate::credentials::schedule_client,
        )
    }

    /// [`Self::open`] over the two builders, so a test can hand in one that
    /// refuses without breaking a real TLS backend.
    fn open_with(
        vendor: impl Fn() -> reqwest::Result<reqwest::Client>,
        schedule: impl FnOnce() -> reqwest::Result<reqwest::Client>,
    ) -> Result<Self, BootFailure> {
        Ok(Self {
            broker: vendor()?,
            plane: vendor()?,
            outbound: vendor()?,
            schedule: schedule()?,
        })
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the restriction set is for the daemon"
    )]

    use super::Clients;
    use crate::error::BootFailure;

    /// A builder failure, raised the way a refused build raises one: a request
    /// whose URL will not parse fails in the same `reqwest::Error` type.
    fn refused() -> reqwest::Result<reqwest::Client> {
        #[expect(
            clippy::disallowed_methods,
            reason = "the client only builds a request that fails to parse; it never sends"
        )]
        let failure = reqwest::Client::new()
            .get("not a url")
            .build()
            .expect_err("a URL with no scheme does not build");
        Err(failure)
    }

    #[test]
    fn a_vendor_client_that_cannot_build_refuses_boot() {
        let refusal = Clients::open_with(refused, crate::credentials::schedule_client)
            .expect_err("a vendor client that will not build refuses boot");

        assert!(matches!(refusal, BootFailure::HttpClient(_)), "{refusal:?}");
        assert_eq!(refusal.phase(), "http_client");
        assert_eq!(
            refusal.code(),
            afd_core::error_code::INTERNAL_OPERATION_FAILED
        );
        assert!(
            std::error::Error::source(&refusal).is_some(),
            "the builder's own reason is the cause a boot log walks to"
        );
    }

    #[test]
    fn a_scheduler_client_that_cannot_build_refuses_boot() {
        let refusal = Clients::open_with(crate::credentials::vendor_exchange_client, refused)
            .expect_err("a scheduler client that will not build refuses boot");

        assert!(matches!(refusal, BootFailure::HttpClient(_)), "{refusal:?}");
    }

    #[test]
    fn the_real_builders_build_every_client() {
        assert!(Clients::open().is_ok(), "the real builders build");
    }
}
