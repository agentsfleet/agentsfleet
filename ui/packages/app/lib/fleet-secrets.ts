// Secret routing for the library-entry install preview.
//
// A library entry's required credentials (`FleetLibraryGalleryEntry.requirements
// .credentials`) are TRIGGER.md vault references — workspace service credentials
// by construction. They are resolved by exact name against the workspace vault,
// so a missing one always routes to the workspace credentials flow. The tenant
// model provider is a separate surface
// (`/settings/models`) and never appears in library-entry requirements, so the
// preview links service credentials here and only here — it must not imply the
// two are the same thing.
//
// `/secrets` is the semantic write-only secret-vault route — a real standalone
// page (Secrets), not a redirect. The deep-link points at this route
// name rather than any page's internal layout, so the preview stays decoupled
// from the vault section and survives further relocation.
export const WORKSPACE_SECRETS_PATH = "/secrets";

// Which required credentials are not yet present in the workspace vault. Match
// is exact, mirroring the backend resolver (vault entry name == requirement
// name); a different-cased vault entry does not satisfy a requirement.
export function missingSecrets(
  required: readonly string[],
  present: readonly string[],
): string[] {
  const have = new Set(present);
  return required.filter((name) => !have.has(name));
}
