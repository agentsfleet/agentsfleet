// `agentsfleet library add` — onboard a Fleet library into this workspace.
//
// The daemon has accepted this body on the workspace plane for some time; the
// CLI never grew the verb, so a tenant library could only be created from the
// dashboard. That left `install --library` pointing at identifiers the terminal
// had no way to produce.
//
// Source selection is resolved to ONE tagged value before any request is built,
// which is what removes the downstream "cannot happen" branches: an upload
// carries its documents because the variant holds them, and a github source
// carries its repository because the variant holds it.

import { readdirSync } from "node:fs";

import { Effect } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { Workspaces } from "../services/workspaces.ts";
import { requireWorkspaceId, resolveAuthToken } from "./workspace-guards.ts";
import { wsFleetLibrariesPath } from "../lib/api-paths.ts";
import { ValidationError, type CliError } from "../errors/index.ts";
import { loadBundle } from "./fleet_install.ts";
import {
  BUNDLE_SKILL_FILE,
  BUNDLE_TRIGGER_FILE,
  LIBRARY_SOURCE_KIND,
} from "../constants/library-source.ts";
import { LIBRARY_ID_PLACEHOLDER } from "../constants/cli-flags.ts";
import { printRequirements, type BundleRequirements } from "./fleet_install_source.ts";

const METHOD_POST = "POST" as const;

/** `owner/repo` — one slash, and neither half may be empty or carry a path
 *  separator. The daemon refuses the same shapes; refusing here costs no
 *  request. */
const REPOSITORY_PATTERN = /^[^/\s]+\/[^/\s]+$/u;
// Both separators: a Windows bundle path reduced by a slash-only splitter is
// not reduced at all, and the whole point of reducing it is to keep an
// operator's home directory off a row their colleagues read.
const PATH_SEPARATORS = /[\\/]/u;
const TRAILING_SEPARATORS = /[\\/]+$/u;

export interface LibraryAddFlags {
  readonly github?: string | undefined;
  readonly from?: string | undefined;
  readonly template?: string | undefined;
  readonly revision?: string | undefined;
}

/** The onboarding body, shaped per source kind. `support_files` is always sent
 *  because the daemon's parse reads it unconditionally; an upload must send it
 *  empty, and a github source has its attachments fetched server-side. */
interface LibraryImportBody {
  readonly source_kind: string;
  readonly source_ref: string;
  readonly ref?: string;
  readonly skill_markdown?: string;
  readonly trigger_markdown?: string;
  readonly support_files: ReadonlyArray<never>;
}

interface LibraryCreatedResponse {
  readonly id?: string | null;
  readonly name?: string | null;
  readonly visibility?: string | null;
  readonly requirements?: BundleRequirements;
}

export const ADD_USAGE =
  "usage: agentsfleet library add (--github <owner/repo> [--ref <rev>] | --from <path> | --template <id>)" as const;
const ONE_SOURCE_REQUIRED =
  "library add needs exactly one of --github, --from, or --template" as const;
const REVISION_NEEDS_GITHUB =
  "--ref names a branch, tag, or commit, so it rides --github only" as const;
const REPOSITORY_SHAPE =
  "--github takes owner/repo, for example agentsfleet/github-pr-reviewer" as const;
const UPLOAD_DROPS_FILES =
  "an upload carries SKILL.md and TRIGGER.md only, and this bundle has more" as const;
const UPLOAD_DROPS_SUGGESTION =
  "onboard it with --github <owner/repo>, which fetches support files server-side, or remove them from the directory" as const;
const UNREADABLE_BUNDLE = "the bundle directory could not be listed" as const;
const DOTFILE_PREFIX = "." as const;

/** What an upload records as its origin.
 *
 *  The daemon stores `source_ref` as provenance and the gallery prints it to
 *  every workspace member, so sending the absolute path would put an operator's
 *  home directory — and their username — on a row their colleagues read. The
 *  bundle's own directory name says where it came from without that.
 */
const uploadProvenance = (path: string): string => {
  const trimmed = path.replace(TRAILING_SEPARATORS, "");
  const name = trimmed.split(PATH_SEPARATORS).pop();
  return name && name.length > 0 ? name : trimmed;
};

/**
 * The bundle files an upload cannot carry.
 *
 * The daemon refuses an upload with attachments outright, so sending them is
 * not an option — but reading only the two root documents and reporting
 * success is worse: the Fleet installs, and the instructions reference files
 * that were never uploaded. `tests/fixtures/fleetbundle/security-reviewer`
 * ships a `checklists/` directory, so this is the ordinary shape, not a corner.
 *
 * A github source fetches attachments server-side, which is why the refusal
 * names it.
 */
const supportFilesIn = (dir: string): ReadonlyArray<string> => {
  const entries = readdirSync(dir, { withFileTypes: true });
  return entries
    .filter(
      (entry) =>
        !entry.name.startsWith(DOTFILE_PREFIX) &&
        entry.name !== BUNDLE_SKILL_FILE &&
        entry.name !== BUNDLE_TRIGGER_FILE,
    )
    .map((entry) => (entry.isDirectory() ? `${entry.name}/` : entry.name));
};

const reject = (detail: string, suggestion: string) =>
  Effect.fail(new ValidationError({ detail, suggestion }));

/** What one resolved source is: a kind the daemon serves and the reference that
 *  kind carries. Returned as one value so no later step can read a second
 *  source or find the kind and the reference disagreeing. */
interface ResolvedSource {
  readonly kind: string;
  readonly ref: string;
}

/** Exactly one source, resolved before anything else runs. */
const selectSource = (
  flags: LibraryAddFlags,
): Effect.Effect<ResolvedSource, ValidationError> => {
  const chosen: ResolvedSource[] = [];
  if (flags.github) chosen.push({ kind: LIBRARY_SOURCE_KIND.github, ref: flags.github });
  if (flags.from) chosen.push({ kind: LIBRARY_SOURCE_KIND.upload, ref: flags.from });
  if (flags.template) chosen.push({ kind: LIBRARY_SOURCE_KIND.template, ref: flags.template });

  const [source] = chosen;
  if (chosen.length !== 1 || source === undefined) {
    return reject(ONE_SOURCE_REQUIRED, ADD_USAGE);
  }
  if (flags.revision && source.kind !== LIBRARY_SOURCE_KIND.github) {
    return reject(REVISION_NEEDS_GITHUB, ADD_USAGE);
  }
  if (
    source.kind === LIBRARY_SOURCE_KIND.github &&
    !REPOSITORY_PATTERN.test(source.ref)
  ) {
    return reject(REPOSITORY_SHAPE, ADD_USAGE);
  }
  return Effect.succeed(source);
};

/** Build the body for the resolved source. An upload reads its bundle here, so
 *  a missing directory fails before a request rather than after one. */
const bodyForSource = (
  source: ResolvedSource,
  flags: LibraryAddFlags,
): Effect.Effect<LibraryImportBody, CliError> =>
  Effect.gen(function* () {
    if (source.kind !== LIBRARY_SOURCE_KIND.upload) {
      return {
        source_kind: source.kind,
        source_ref: source.ref,
        ...(flags.revision ? { ref: flags.revision } : {}),
        support_files: [],
      };
    }
    const bundle = yield* loadBundle(source.ref);
    const extras = yield* Effect.try({
      try: () => supportFilesIn(source.ref),
      // A directory that cannot be listed is the bundle loader's problem, and
      // it already failed above if the path is unusable.
      catch: () => new ValidationError({ detail: UNREADABLE_BUNDLE, suggestion: ADD_USAGE }),
    });
    if (extras.length > 0) {
      return yield* reject(
        `${UPLOAD_DROPS_FILES}: ${extras.join(", ")}`,
        UPLOAD_DROPS_SUGGESTION,
      );
    }
    return {
      source_kind: LIBRARY_SOURCE_KIND.upload,
      // The path is provenance, not a fetch instruction: the daemon stores it
      // so a row says where its bytes came from, and reads the documents from
      // the body.
      source_ref: uploadProvenance(source.ref),
      skill_markdown: bundle.skill_md,
      ...(bundle.trigger_md ? { trigger_markdown: bundle.trigger_md } : {}),
      support_files: [],
    };
  });

export const libraryAddEffectFromFlags = (
  flags: LibraryAddFlags,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;

    const source = yield* selectSource(flags);
    const body = yield* bodyForSource(source, flags);
    const workspaceId = yield* requireWorkspaceId;
    const token = yield* resolveAuthToken;

    const res = yield* http.request<LibraryCreatedResponse>({
      path: wsFleetLibrariesPath(workspaceId),
      method: METHOD_POST,
      body,
      token,
    });

    if (config.jsonMode) {
      yield* output.printJson(res);
      return;
    }
    yield* output.success(`${res.name ?? source.ref} onboarded.`);
    if (res.id) yield* output.info(`  Library ID: ${res.id}`);
    // The same requirement preview `install` prints, for the same reason: the
    // secrets a bundle declares are what the operator has to supply next.
    yield* printRequirements(res.requirements);
    yield* output.info(`  Install it with: agentsfleet install --library ${res.id ?? LIBRARY_ID_PLACEHOLDER}`);
  });
