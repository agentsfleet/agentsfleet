import RouteLoading from "@/components/layout/RouteLoading";

// Secrets is its own standalone page now — it no longer redirects to
// /settings/models, so paint its real header. Without this it borrowed the stale
// "Models" title and flashed "Loading Models…" before the Secrets page resolved.
export default function SecretsLoading() {
  return (
    <RouteLoading
      title="Secrets"
      description="Encrypted secrets your fleets can use — write-only once saved."
    />
  );
}
