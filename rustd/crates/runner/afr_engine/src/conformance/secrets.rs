//! No secret enters a sandbox: after a checkout fetched with a canary token,
//! neither the token nor the credentials it was presented as is in any file
//! the sandbox can write, any process's environment or any command line, and
//! the same search finds a canary planted on purpose.

use bytes::Bytes;
use libtest_mimic::Failed;

use super::Context;
use super::run::{build, expect, said, within};
use crate::engine::{Engine, Limits};

/// A token shaped like an installation token: the checkout fetches with it,
/// and nothing inside the sandbox may hold it.
pub(super) const CANARY: &str = "ghs_conformanceCanaryNeverInside";
/// Where the canary is planted on purpose, and the path the search names it
/// by.
const PLANTED: &str = "planted";
pub(super) const PLANTED_PATH: &str = "/workspace/planted";

/// Lists every file under the sandbox's writable trees, and every process's
/// environment and command line, that holds the canary or the basic
/// credentials it would be presented as. The canary is spelled in two halves
/// joined in an unexported variable, and the patterns reach `grep` on its
/// input, so no command line or environment of the search itself holds it.
pub(super) fn search() -> String {
    let (head, tail) = CANARY.split_at(CANARY.len() / 2);
    format!(
        "a='{head}'; b='{tail}'; t=\"$a$b\"; \
         e=$(printf 'x-access-token:%s' \"$t\" | base64 | tr -d '\\n'); \
         printf '%s\\n%s\\n' \"$t\" \"$e\" | grep -rlsF -D skip --exclude-dir=lost+found -f - \
         /workspace /tmp /run /etc /home /root /var /proc/[0-9]*/environ /proc/[0-9]*/cmdline; true"
    )
}

/// A checkout with the canary leaves no trace of it; planted, it is found.
pub(super) async fn no_secret_in_sandbox(
    engine: &dyn Engine,
    context: &Context<'_>,
) -> Result<(), Failed> {
    let (after_checkout, after_planting) =
        within(build(engine, Limits::DEFAULT).await?, async |sandbox| {
            context.checkout.check_out(sandbox, CANARY).await?;
            let after_checkout = said(sandbox, &search()).await?;
            sandbox
                .agent()
                .write_file(PLANTED, Bytes::from_static(CANARY.as_bytes()))
                .await?;
            Ok((after_checkout, said(sandbox, &search()).await?))
        })
        .await?;
    expect(
        after_checkout.is_empty(),
        format!("nothing holds the token, got {after_checkout:?}"),
    )?;
    expect(
        after_planting == PLANTED_PATH,
        format!("the search finds the planted canary alone, got {after_planting:?}"),
    )
}
