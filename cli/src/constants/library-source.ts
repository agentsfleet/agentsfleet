// Fleet-library onboarding source kinds.
//
// The daemon parses `source_kind` into a closed set of three and refuses
// anything else, so the client carries the same three and refuses locally
// rather than spending a request to be told (RULE UFS: one spelling, shared
// with the tests).

export const LIBRARY_SOURCE_KIND = {
  github: "github",
  upload: "upload",
  template: "template",
} as const;

// The bundle documents an upload carries. The daemon reads `SKILL.md` as the
// root document and treats `TRIGGER.md` as optional; a directory without the
// former is not a bundle.
export const BUNDLE_SKILL_FILE = "SKILL.md" as const;
export const BUNDLE_TRIGGER_FILE = "TRIGGER.md" as const;

