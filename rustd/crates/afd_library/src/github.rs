//! GitHub tarball transport and safe archive extraction.

use std::io::Read as _;
use std::time::Duration;

use flate2::read::GzDecoder;
use futures_util::StreamExt as _;

use crate::{BundleSource, Error, ImportBody, Result, SourceFailure, SourceKind, SupportFile};

const API_HOST: &str = "api.github.com";
const CODELOAD_HOST: &str = "codeload.github.com";
const USER_AGENT: &str = "agentsfleetd";
const SKILL_PATH: &str = "SKILL.md";
const TRIGGER_PATH: &str = "TRIGGER.md";
const PARENT_SEGMENT: &str = "..";
const MAX_SEGMENT_LEN: usize = 100;
const MAX_COMPRESSED_BYTES: usize = 8 * 1024 * 1024;
const MAX_EXPANDED_BYTES: u64 = 16 * 1024 * 1024;
const MAX_TAR_ENTRIES: usize = 4096;
const MAX_ENTRY_BYTES: u64 = 200 * 1024;

/// How long the tarball fetch may spend reaching GitHub.
///
/// A deadline at the call site, per Invariant 4, and this is the call site: no
/// caller above holds one. Without it a GitHub that accepts a connection and
/// then says nothing parks an onboarding request forever — `reqwest`'s default
/// is no timeout at all, and the size caps above bound only how much this
/// reads, never how long it waits to read it.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// How long the whole fetch may take, connection included.
///
/// Generous next to [`CONNECT_TIMEOUT`] because it covers a real download of up
/// to [`MAX_COMPRESSED_BYTES`] over whatever link the daemon has, and the two
/// differ for that reason: an unreachable host is decided in seconds, a slow
/// one is given the minute a large bundle honestly needs.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// A validated GitHub `owner/repository` pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repository {
    owner: String,
    name: String,
}

impl Repository {
    /// Parses exactly one slash and safe URL-segment characters.
    ///
    /// # Errors
    /// Refuses empty, traversal, overlong, and injection-shaped segments.
    pub fn parse(reference: &str) -> Result<Self> {
        let Some((owner, name)) = reference.split_once('/') else {
            return Err(SourceFailure::InvalidReference.into());
        };
        if name.contains('/') || !valid_segment(owner) || !valid_segment(name) {
            return Err(SourceFailure::InvalidReference.into());
        }
        Ok(Self {
            owner: owner.into(),
            name: name.into(),
        })
    }
}

/// Whether a branch, tag, or commit is one safe URL segment.
#[must_use]
pub fn valid_revision(value: &str) -> bool {
    valid_segment(value)
}

/// Production GitHub source with redirects disabled and inspected explicitly.
#[derive(Debug, Clone)]
pub struct GithubSource {
    client: reqwest::Client,
    api_base: String,
    revision: String,
}

impl GithubSource {
    /// Builds a source for one branch, tag, or commit spelling.
    ///
    /// # Errors
    /// Refuses an unsafe revision or a client-construction failure.
    pub fn new(revision: impl Into<String>) -> Result<Self> {
        let revision = revision.into();
        if !valid_segment(&revision) {
            return Err(SourceFailure::InvalidReference.into());
        }
        let client = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(Error::Github)?;
        Ok(Self {
            client,
            api_base: format!("https://{API_HOST}"),
            revision,
        })
    }

    /// Redirects the first request to a test-owned HTTP origin.
    #[cfg(any(test, feature = "test-util"))]
    pub(crate) fn pointed_at(mut self, api_base: impl Into<String>) -> Self {
        self.api_base = api_base.into();
        self
    }

    async fn download(&self, repository: &Repository) -> Result<Vec<u8>> {
        self.download_with(repository, validate_redirect).await
    }

    async fn download_with(
        &self,
        repository: &Repository,
        redirect_allowed: fn(&str) -> Result<()>,
    ) -> Result<Vec<u8>> {
        let url = format!(
            "{}/repos/{}/{}/tarball/{}",
            self.api_base, repository.owner, repository.name, self.revision
        );
        let first = self.send(&url).await?;
        if first.status().is_redirection() {
            let location = first
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or(SourceFailure::DisallowedRedirect)?;
            redirect_allowed(location)?;
            let second = self.send(location).await?;
            if second.status().is_redirection() {
                return Err(SourceFailure::DisallowedRedirect.into());
            }
            return response_bytes(second).await;
        }
        response_bytes(first).await
    }

    async fn send(&self, url: &str) -> Result<reqwest::Response> {
        self.client
            .get(url)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .send()
            .await
            .map_err(Error::Github)
    }
}

impl BundleSource for GithubSource {
    async fn fetch(&self, reference: &str) -> Result<ImportBody> {
        let repository = Repository::parse(reference)?;
        let compressed = self.download(&repository).await?;
        let reference = reference.to_owned();
        let revision = self.revision.clone();
        tokio::task::spawn_blocking(move || extract(&compressed, &reference, &revision))
            .await
            .map_err(Error::ArchiveTask)?
    }
}

async fn response_bytes(response: reqwest::Response) -> Result<Vec<u8>> {
    if let Some(failure) = classify_status(response.status().as_u16()) {
        return Err(failure.into());
    }
    let capacity = response
        .content_length()
        .and_then(|length| usize::try_from(length).ok())
        .map_or(0, |length| length.min(MAX_COMPRESSED_BYTES));
    let mut response = response
        .error_for_status()
        .map_err(Error::Github)?
        .bytes_stream();
    let mut bytes = Vec::with_capacity(capacity);
    while let Some(chunk) = response.next().await {
        let chunk = chunk.map_err(Error::Github)?;
        if bytes.len().saturating_add(chunk.len()) > MAX_COMPRESSED_BYTES {
            return Err(SourceFailure::ArchiveTooLarge.into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn classify_status(status: u16) -> Option<SourceFailure> {
    match status {
        404 => Some(SourceFailure::NotFound),
        403 | 429 => Some(SourceFailure::RateLimited),
        _ => None,
    }
}

fn validate_redirect(location: &str) -> Result<()> {
    let url = reqwest::Url::parse(location).map_err(Error::Redirect)?;
    let allowed_host = matches!(url.host_str(), Some(API_HOST | CODELOAD_HOST));
    if url.scheme() == "https" && allowed_host {
        Ok(())
    } else {
        Err(SourceFailure::DisallowedRedirect.into())
    }
}

fn valid_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SEGMENT_LEN
        && value != "."
        && value != PARENT_SEGMENT
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn extract(compressed: &[u8], reference: &str, revision: &str) -> Result<ImportBody> {
    if compressed.is_empty() || compressed.len() > MAX_COMPRESSED_BYTES {
        return Err(SourceFailure::Truncated.into());
    }
    let mut decoder = GzDecoder::new(compressed).take(MAX_EXPANDED_BYTES + 1);
    let mut expanded = Vec::with_capacity(compressed.len());
    decoder.read_to_end(&mut expanded).map_err(Error::Archive)?;
    if u64::try_from(expanded.len()).is_err() || expanded.len() as u64 > MAX_EXPANDED_BYTES {
        return Err(SourceFailure::ArchiveTooLarge.into());
    }
    extract_tar(&expanded, reference, revision)
}

fn extract_tar(bytes: &[u8], reference: &str, revision: &str) -> Result<ImportBody> {
    let mut skill = None;
    let mut trigger = None;
    let mut support_files = Vec::with_capacity(crate::validate::MAX_SUPPORT_FILES);
    let mut archive = tar::Archive::new(bytes);
    let entries = archive.entries().map_err(Error::Archive)?;
    for (index, entry) in entries.enumerate() {
        if index >= MAX_TAR_ENTRIES {
            return Err(SourceFailure::TooManyFiles.into());
        }
        let mut entry = entry.map_err(Error::Archive)?;
        if entry.header().entry_type().is_symlink() || entry.header().entry_type().is_hard_link() {
            return Err(SourceFailure::UnsafeArchive.into());
        }
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = safe_relative(&entry.path_bytes())?;
        let Some(path) = path else { continue };
        if entry.size() > MAX_ENTRY_BYTES {
            return Err(SourceFailure::ArchiveTooLarge.into());
        }
        let capacity =
            usize::try_from(entry.size()).map_err(|_overflow| SourceFailure::ArchiveTooLarge)?;
        let mut content = Vec::with_capacity(capacity);
        entry.read_to_end(&mut content).map_err(Error::Archive)?;
        match path.as_str() {
            SKILL_PATH if skill.is_none() => skill = Some(content),
            TRIGGER_PATH if trigger.is_none() => trigger = Some(content),
            SKILL_PATH | TRIGGER_PATH => return Err(SourceFailure::UnsafeArchive.into()),
            _ => support_files.push(SupportFile { path, content }),
        }
    }
    Ok(ImportBody {
        source_kind: SourceKind::Github,
        source_ref: reference.into(),
        source_revision: Some(revision.into()),
        skill_markdown: skill.ok_or(SourceFailure::Truncated)?,
        trigger_markdown: trigger,
        support_files,
    })
}

fn safe_relative(raw: &[u8]) -> Result<Option<String>> {
    let path = core::str::from_utf8(raw).map_err(Error::ArchivePath)?;
    if path.starts_with('/') || path.contains('\\') || path.contains('\0') {
        return Err(SourceFailure::UnsafeArchive.into());
    }
    let mut segments = path.split('/');
    let _wrapper = segments.next().ok_or(SourceFailure::UnsafeArchive)?;
    let relative: Vec<_> = segments.collect();
    if relative.is_empty()
        || relative
            .iter()
            .any(|part| part.is_empty() || *part == PARENT_SEGMENT)
    {
        return Err(SourceFailure::UnsafeArchive.into());
    }
    if relative.iter().any(|part| part.starts_with('.')) {
        return Ok(None);
    }
    Ok(Some(relative.join("/")))
}

#[cfg(test)]
mod tests;
