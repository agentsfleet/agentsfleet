import RouteLoading from "@/components/layout/RouteLoading";
import { SECRETS_PAGE_DESCRIPTION, SECRETS_PAGE_TITLE } from "./copy";

// Secrets is its own standalone page now — it no longer redirects to
// /settings/models, so paint its real header. Without this it borrowed the stale
// "Models" title and flashed "Loading Models…" before the Secrets page resolved.
export default function SecretsLoading() {
  return (
    <RouteLoading
      title={SECRETS_PAGE_TITLE}
      description={SECRETS_PAGE_DESCRIPTION}
    />
  );
}
