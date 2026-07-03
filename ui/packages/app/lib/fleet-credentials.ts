// Credential routing for the template install preview.
//
// A template's required credentials (`FleetTemplateGalleryEntry.requirements
// .credentials`) are TRIGGER.md vault references — workspace service credentials
// by construction. They are resolved by exact name against the workspace vault,
// so a missing one always routes to the workspace credentials flow. The tenant
// model provider is a separate surface
// (`/settings/models`) and never appears in template requirements, so the preview
// links service credentials here and only here — it must not imply the two are
// the same thing.
//
// `/credentials` is the semantic write-only secret-vault route. The standalone
// vault page was folded into Models, so `/credentials` now
// `redirect()`s to `/settings/models` (whose custom-secrets section IS the vault
// surface). The deep-link keeps pointing at the stable `/credentials` route
// rather than the moving destination, so the preview stays decoupled from the
// page's layout and survives any further relocation of the vault section.
export const WORKSPACE_CREDENTIALS_PATH = "/credentials";

// Which required credentials are not yet present in the workspace vault. Match
// is exact, mirroring the backend resolver (vault entry name == requirement
// name); a different-cased vault entry does not satisfy a requirement.
export function missingCredentials(
  required: readonly string[],
  present: readonly string[],
): string[] {
  const have = new Set(present);
  return required.filter((name) => !have.has(name));
}
