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

export const SAMPLE_LIBRARY_REPO = "agentsfleet/github-pr-reviewer";

export const LIBRARY_DOC_URL = "https://docs.agentsfleet.net/fleets/library#writing-your-own";

// owner/repo — the only source form the platform surface accepts.
export const SOURCE_REF_PATTERN = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/;
