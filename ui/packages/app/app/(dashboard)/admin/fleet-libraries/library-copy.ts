// Copy and routing constants for the platform fleet-library surface, kept in a
// neutral module so the server page and the client dialog share one spelling.

export const FLEET_LIBRARIES_TITLE = "Fleet libraries";

export const FLEET_LIBRARIES_DESCRIPTION =
  "Onboard a fleet from its GitHub repository. Every workspace can install what lands here.";

// Where a caller without `platform-library:write` is sent. Mirrors the notice
// shape the models surface uses for the same class of redirect.
export const NOT_PLATFORM_ADMIN = "/settings?notice=fleet-libraries-platform-admin-only";

export const ONBOARD_ACTION = "onboard the fleet library";

export const ONBOARD_TOOLTIP = "Onboard a fleet into the platform catalog from GitHub.";

// The source contract (accepted form, example repository, authoring docs) is
// shared with the workspace onboarding dialog — see lib/fleet-library-source.ts.
// It is deliberately NOT re-spelled here: both surfaces feed the same importer,
// so a tightening must reach both.
export {
  SOURCE_REF_PATTERN,
  SAMPLE_LIBRARY_REPO,
  LIBRARY_AUTHORING_DOC_URL,
} from "@/lib/fleet-library-source";
