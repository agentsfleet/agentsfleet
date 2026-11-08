//! Database-scoped handles keep cleanup ownership explicit.

use super::{API_KNOB, LANE_KNOB, TestDatabase};

impl TestDatabase {
    /// The connection URL this handle's tests run against.
    ///
    /// The lane's URL unchanged for a shared handle. For an owned one, the same
    /// URL with the database swapped — keeping whatever query string the lane
    /// carried, `sslmode` among them, which a lane against a TLS Postgres needs
    /// and a rebuilt URL would silently drop.
    ///
    /// Swapped through a parse rather than at the last `/`: a query carrying a
    /// path of its own (`sslrootcert=/etc/ca.pem`) has a later `/` than the
    /// database's, and a lane URL naming no database has none at all.
    ///
    /// # Panics
    /// When the lane's URL is not a URL, which is a lane fault.
    #[must_use]
    pub fn url(&self) -> String {
        self.on_this_database(&self.base_url, LANE_KNOB)
    }

    /// [`TestDatabase::url`] through the API login: what [`DbRole::Api`]
    /// connects as.
    ///
    /// # Panics
    /// When the API lane URL is not a URL, which is a lane fault.
    #[must_use]
    pub fn api_url(&self) -> String {
        self.on_this_database(&self.api_base_url, API_KNOB)
    }

    /// Opens the same database without transferring its cleanup ownership.
    /// A caller can retain this owner while guarded fixture preparation runs.
    ///
    /// # Panics
    /// When either lane URL is malformed.
    #[must_use]
    pub fn shared_handle(&self) -> Self {
        Self {
            base_url: self.url(),
            api_base_url: self.api_url(),
            owned: None,
        }
    }

    /// `lane` with this handle's database swapped in, when it owns one.
    fn on_this_database(&self, lane: &str, knob: &str) -> String {
        let Some(name) = self.owned.as_deref() else {
            return lane.to_owned();
        };
        let mut url = url::Url::parse(lane)
            .unwrap_or_else(|failure| panic!("{knob} must be a URL: {failure}"));
        url.set_path(&format!("/{name}"));
        url.into()
    }

    /// The database this handle created, when it created one.
    ///
    /// `None` for the shared lane database — which is the point: a caller that
    /// wants to DROP a database has to get a name out of this, and the shared
    /// handle has none to give.
    #[must_use]
    pub fn database_name(&self) -> Option<&str> {
        self.owned.as_deref()
    }
}
