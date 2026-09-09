//! What the lane puts in the database before a test runs.
//!
//! Split from [`super`] at the seam the file already had: everything here
//! WRITES fixture rows, and everything left there opens the stores and reads
//! them back. The split happened when the credential seed stopped being a name
//! and became a sealed envelope — the install classifies a declared credential
//! by opening one, so the fixture had to grow a real vault write.

use std::sync::Arc;

use afd_crypto::entropy::Entropy;
use afd_crypto::secret::Kek;

use super::{FIXTURE_KEK, Lane, NOW_MS, STATIC_HANDLE, VISIBILITY_PUBLIC};

impl Lane {
    /// Seeds one platform library entry, idempotently.
    ///
    /// `ON CONFLICT (id) DO NOTHING`, because every lane seeds this row into one
    /// shared database. Whichever lane runs FIRST therefore decides the stored
    /// content — correct for [`LIBRARY_ID`], which is identical every time, and
    /// wrong for a caller wanting different markdown under a reused id.
    pub(crate) async fn seed_library_entry(
        &self,
        id: &str,
        skill_markdown: &str,
        trigger_markdown: Option<&str>,
    ) {
        sqlx::query(
            "INSERT INTO core.fleet_library \
               (id, name, description, source_repo, source_path, source_ref, \
                required_credentials, required_credentials_reasons, required_tools, \
                network_hosts, visibility, content_hash, skill_markdown, trigger_markdown, \
                created_at, updated_at) \
             VALUES ($1, $1, 'fixture', 'repo', 'path', 'main', \
                     '[]'::jsonb, '{}'::jsonb, '[]'::jsonb, '[]'::jsonb, \
                     $2, $3, $4, $5, $6, $6) \
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(id)
        .bind(VISIBILITY_PUBLIC)
        .bind(format!("sha256:{id}"))
        .bind(skill_markdown)
        .bind(trigger_markdown)
        .bind(NOW_MS)
        .execute(&mut *self.connection().await)
        .await
        .expect("seeding a library entry");
    }

    /// Stores one credential under `key_name`, sealed so it can be OPENED.
    ///
    /// A real envelope rather than the placeholder bytes this used to write.
    /// The pre-flight only ever read names, and placeholders were honest about
    /// that; the install now classifies each declared credential by opening its
    /// handle, and a handle that cannot be opened is indistinguishable from one
    /// that names no integration — so every seeded secret would silently be a
    /// static one and the grant request could never be proven.
    ///
    /// `body` is the stored handle. `{"integration":"github",…}` is the shape
    /// that makes a credential mintable; anything else ships as it stands.
    pub(crate) async fn seal_secret(&self, key_name: &str, body: &str) {
        let stored = serde_json::from_str::<Box<serde_json::value::RawValue>>(body)
            .expect("the fixture handle must be JSON");
        afd_vault::Vault::new(
            self.pool.clone(),
            Arc::new(Kek::from_bytes(FIXTURE_KEK)),
            Entropy::new(),
        )
        .create(
            &self.workspace,
            &afd_vault::SecretName::parse(key_name).expect("the fixture name must be storable"),
            &afd_vault::SecretBody::parse(&stored).expect("the fixture handle must be storable"),
            Self::now(),
        )
        .await
        .expect("sealing a workspace secret");
    }

    /// Stores one STATIC credential under `key_name`.
    ///
    /// The shape every pre-flight fixture wants: present, openable, and not
    /// mintable, so an install seeded through it requests no grant.
    pub(crate) async fn seed_secret(&self, key_name: &str) {
        self.seal_secret(key_name, STATIC_HANDLE).await;
    }

    /// Stores one credential name with UNOPENABLE envelope bytes.
    ///
    /// Kept for the one claim it still serves: a row exists, so the pre-flight's
    /// set difference is satisfied, and nothing can read what it holds.
    pub(crate) async fn seed_unopenable_secret(&self, key_name: &str) {
        sqlx::query(
            "INSERT INTO vault.secrets \
               (id, workspace_id, key_name, kek_version, \
                encrypted_dek, dek_nonce, dek_tag, nonce, ciphertext, tag, \
                created_at, updated_at) \
             VALUES ($1::uuid, $2::uuid, $3, 1, \
                     '\\x00', '\\x00', '\\x00', '\\x00', '\\x00', '\\x00', \
                     $4, $4)",
        )
        .bind(afd_db::test_util::mint_id())
        .bind(self.workspace.as_str())
        .bind(key_name)
        .bind(NOW_MS)
        .execute(&mut *self.connection().await)
        .await
        .expect("seeding a workspace secret");
    }
}
