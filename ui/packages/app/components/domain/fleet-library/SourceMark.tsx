import { PackageIcon, UploadIcon } from "lucide-react";
import { VendorMark } from "./VendorMark";
import { SOURCE_REF_PATTERN } from "@/lib/fleet-library-source";

// Where a bundle came from, drawn once for both fleet-library tables — the
// workspace's own onboarded entries and the platform catalogue.
//
// The kind used to be spelled into the cell as a prefix (`upload:probe/skill-only`,
// `github:agentsfleet/github-pr-reviewer`), which cost a word on every row to say
// something a glyph says at a glance. The glyph carries the kind; the text carries
// only the ref, which is the part that differs between two rows of the same kind.
//
// The kind is never rendered as a word, so every caller must keep the column
// header naming the set ("Source"). The mark's tooltip names the kind for anyone
// who does not recognise it, and the accessible name of the link says it too.

/** The stored spellings of `source_kind` — `afd_library::model::SourceKind`. */
export const SOURCE_KIND_GITHUB = "github";
export const SOURCE_KIND_UPLOAD = "upload";
export const SOURCE_KIND_TEMPLATE = "template";

export const SOURCE_UPLOAD_TITLE = "Uploaded bundle";
export const SOURCE_TEMPLATE_TITLE = "First-party template";
export const SOURCE_GITHUB_LINK_LABEL = "Open on GitHub";

/** A ref that is a moving target, so the link says so rather than implying a pin. */
export const SOURCE_DRIFT_NOTE =
  "Links the branch as it stands now. The catalogue stores the branch name, not the commit it was fetched at, so this can have moved since.";

const GITHUB_HOST = "https://github.com/";

export type SourceMarkProps = {
  /** `github`, `upload`, or `template` — as the row stores it. */
  readonly kind: string;
  /** What the kind points at: `owner/repo` for GitHub, a path for an upload. */
  readonly sourceRef: string;
  /** The branch or tag a GitHub row was fetched at, where the row carries one. */
  readonly gitRef?: string;
};

/** `https://github.com/owner/repo`, at a ref when the row stores one. */
export function githubSourceUrl(repo: string, gitRef?: string): string {
  const base = `${GITHUB_HOST}${repo}`;
  return gitRef ? `${base}/tree/${gitRef}` : base;
}

export function SourceMark({ kind, sourceRef, gitRef }: SourceMarkProps) {
  // An upload stores a local path, and a template stores a template name.
  // Neither is a repository, so neither becomes a link — a link to
  // `github.com/probe/skill-only` points at nothing.
  const linkable = kind === SOURCE_KIND_GITHUB && SOURCE_REF_PATTERN.test(sourceRef);
  const shown = linkable && gitRef ? `${sourceRef}@${gitRef}` : sourceRef;

  return (
    <span className="flex items-center gap-2 text-sm">
      <SourceGlyph kind={kind} />
      {linkable ? (
        <a
          href={githubSourceUrl(sourceRef, gitRef)}
          target="_blank"
          rel="noopener noreferrer"
          aria-label={`${SOURCE_GITHUB_LINK_LABEL}: ${shown}`}
          title={gitRef ? SOURCE_DRIFT_NOTE : undefined}
          className="text-primary underline-offset-4 hover:underline"
        >
          {shown}
        </a>
      ) : (
        <span className="text-muted-foreground">{shown}</span>
      )}
    </span>
  );
}

/** The kind, as a glyph. `title` is the only place the kind is spelled. */
function SourceGlyph({ kind }: { kind: string }) {
  if (kind === SOURCE_KIND_GITHUB) {
    return (
      <span title={SOURCE_GITHUB_LINK_LABEL} className="shrink-0 text-muted-foreground">
        <VendorMark credential={SOURCE_KIND_GITHUB} />
      </span>
    );
  }
  if (kind === SOURCE_KIND_TEMPLATE) {
    return (
      <span title={SOURCE_TEMPLATE_TITLE} className="shrink-0 text-muted-foreground">
        <PackageIcon size={16} aria-hidden="true" />
      </span>
    );
  }
  return (
    <span title={SOURCE_UPLOAD_TITLE} className="shrink-0 text-muted-foreground">
      <UploadIcon size={16} aria-hidden="true" />
    </span>
  );
}

export default SourceMark;
