// The source half of both fleet-library onboarding dialogs — the workspace one
// (`library:write`) and the platform one (`platform-library:write`). Both post to
// the same importer through `resolve.resolve`, so the shape they may send is one
// shape; keeping it here is what stops the two surfaces from disagreeing about
// what the daemon will accept.
//
// The wire constants and the repository pattern stay in lib/fleet-library-source.ts:
// server components and Playwright specs import those, and neither should pull
// zod in to read a regular expression.

import { z } from "zod";
import { SOURCE_KIND_GITHUB, SOURCE_KIND_UPLOAD, type OnboardLibraryEntryRequest } from "@/lib/types";
import { SAMPLE_LIBRARY_REPO, SOURCE_REF_PATTERN } from "@/lib/fleet-library-source";
import { SKILL_FILE_NAME, TRIGGER_FILE_NAME } from "./bundle-files";

export const SKILL_REQUIRED = `Add the ${SKILL_FILE_NAME} body`;
export const TRIGGER_REQUIRED = `Add the ${TRIGGER_FILE_NAME} body`;
export const SOURCE_REF_MALFORMED = `Use owner/repo, for example ${SAMPLE_LIBRARY_REPO}`;

// One flat shape rather than a discriminated union: react-hook-form registers
// fields by name, and a union whose branches carry different names leaves the
// inactive branch's inputs unregistered between tab switches. `source_kind`
// selects which fields are required instead.
export const librarySourceSchema = z
  .object({
    source_kind: z.enum([SOURCE_KIND_GITHUB, SOURCE_KIND_UPLOAD]),
    source_ref: z.string().trim(),
    skill_markdown: z.string(),
    trigger_markdown: z.string(),
  })
  .superRefine((values, ctx) => {
    if (values.source_kind === SOURCE_KIND_GITHUB) {
      if (!SOURCE_REF_PATTERN.test(values.source_ref)) {
        ctx.addIssue({ code: z.ZodIssueCode.custom, path: ["source_ref"], message: SOURCE_REF_MALFORMED });
      }
      return;
    }
    // Both bodies are required together. A bundle uploaded without its
    // TRIGGER.md installs as a fleet that declares no tools, no credentials and
    // no gate — and the runtime reads an absent gate as "approve everything".
    if (values.skill_markdown.trim().length === 0) {
      ctx.addIssue({ code: z.ZodIssueCode.custom, path: ["skill_markdown"], message: SKILL_REQUIRED });
    }
    if (values.trigger_markdown.trim().length === 0) {
      ctx.addIssue({ code: z.ZodIssueCode.custom, path: ["trigger_markdown"], message: TRIGGER_REQUIRED });
    }
  });

export type LibrarySourceValues = z.infer<typeof librarySourceSchema>;

export const EMPTY_LIBRARY_SOURCE: LibrarySourceValues = {
  source_kind: SOURCE_KIND_GITHUB,
  source_ref: "",
  skill_markdown: "",
  trigger_markdown: "",
};

/** Platform-tier additions. Neither reaches the workspace endpoint, which offers neither. */
export type LibrarySourceExtras = {
  /**
   * The pin a Fetch-update honors. GitHub only — `resolveUpload` refuses a
   * request carrying one with InvalidSourceRef, because pasted bytes came from
   * no revision and a stored ref would be one the content never came from.
   */
  ref?: string;
  /** Overwrite a catalog id already owned by a different source. */
  replace?: boolean;
};

/**
 * Map validated form values onto the importer's request body.
 *
 * The upload branch omits `source_ref` entirely rather than sending the empty
 * string the form holds: an uploaded bundle came from no repository, and the
 * catalog table (`PlatformCatalogTable.tsx`) renders a non-slug source as plain
 * text precisely so such a row never advertises a repository to click through to.
 */
export function librarySourcePayload(
  values: LibrarySourceValues,
  extras: LibrarySourceExtras = {},
): OnboardLibraryEntryRequest {
  const replace = extras.replace ? { replace: true } : {};
  if (values.source_kind === SOURCE_KIND_UPLOAD) {
    return {
      source_kind: SOURCE_KIND_UPLOAD,
      skill_markdown: values.skill_markdown,
      trigger_markdown: values.trigger_markdown,
      ...replace,
    };
  }
  return {
    source_kind: SOURCE_KIND_GITHUB,
    source_ref: values.source_ref,
    ...(extras.ref ? { ref: extras.ref } : {}),
    ...replace,
  };
}
