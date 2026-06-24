// Credential routing for the Fleet Bundle install preview.
//
// A bundle's required credentials (`requirements.credentials` /
// `FleetTemplate.required_credentials`) are TRIGGER.md vault references —
// workspace service credentials by construction. They are resolved by exact
// name against the workspace vault, so a missing one always routes to the
// workspace credentials flow. The tenant model provider is a separate surface
// (`/settings/models`) and never appears in bundle requirements, so the preview
// links service credentials here and only here — it must not imply the two are
// the same thing.
//
// `/credentials` is the top-level write-only secret vault page; linking the
// semantic route keeps the preview decoupled from that page's layout. (The
// model-provider `/settings/models` page is the WRONG target — asserted against
// by the install preview's routing test.)
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
