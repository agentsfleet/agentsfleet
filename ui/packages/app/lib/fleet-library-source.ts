// The GitHub source a fleet-library bundle is onboarded from. Shared by both
// onboarding paths — the workspace dialog (`library:write`) and the platform
// dialog (`platform-library:write`) — so the two can never disagree about what
// the importer will accept. A tightening here reaches both surfaces at once.

/** `owner/repo` — the only source form either onboarding surface accepts. */
export const SOURCE_REF_PATTERN = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/;

/** One branch/tag segment — the charset the server's segment rules enforce. */
export const SOURCE_SEGMENT_PATTERN = /^[A-Za-z0-9_.-]+$/;

/** Shown as the example in both dialogs; a real, importable first-party bundle. */
export const SAMPLE_LIBRARY_REPO = "agentsfleet/github-pr-reviewer";

export const SAMPLE_LIBRARY_REPO_URL = `https://github.com/${SAMPLE_LIBRARY_REPO}`;

/** Where "Learn more" goes — authoring a bundle, not the catalog explainer. */
export const LIBRARY_AUTHORING_DOC_URL = "https://docs.agentsfleet.net/fleets/authoring";
