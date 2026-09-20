// The flag vocabulary the command tree is built from.
//
// One declaration per thing a person can type, because the same flag appears
// on many commands and the wording of its refusal is part of the interface.
// `--fleet <id>` is declared once here and reused by `grant list`, `logs`,
// `memory list` and the rest, rather than each command re-deriving what a
// Fleet id is and what to say when it is not one.
//
// This replaces `program/validators.ts`, whose parsers threw commander's
// `InvalidArgumentError`. `effect/unstable/cli` asks for the check as a
// predicate plus the sentence to print, so the sentence lives beside the rule
// it belongs to instead of inside a thrown object.

import { Flag, Argument } from "effect/unstable/cli";
import { EXAMPLE_UUIDV7, isValidId } from "../../lib/id.ts";
import { LIBRARY_ID_PLACEHOLDER } from "../../constants/cli-flags.ts";
import {
  HTTPS_SCHEME_PREFIX,
  OPENAI_COMPATIBLE_PROVIDER,
} from "../../constants/custom-endpoint.ts";
import { API_KEY_SORTS } from "../../constants/api-key.ts";
import { MAX_RECALL_LIMIT } from "../../constants/memory-limits.ts";

const NOT_A_UUIDV7 = `expected uuidv7 format (e.g. ${EXAMPLE_UUIDV7})` as const;

// A flag name is the wire form a person types, so each is written once and
// shared by every variant that spells it — `--limit` means the same thing on
// four commands even though each bounds it differently.
const FLAG = {
  cron: "cron",
  data: "data",
  from: "from",
  limit: "limit",
  message: "message",
  model: "model",
  name: "name",
  provider: "provider",
  timezone: "timezone",
} as const;

const DESC_CRON = "Cron expression" as const;
const DESC_MESSAGE = "Message sent to the Fleet" as const;
const WORKSPACE_ID_DESC = "Workspace ID" as const;
const FLEET_ID_DESC = "Fleet ID" as const;
const NEXT_CURSOR = "next_cursor from a previous page" as const;
const PAGE_SIZE = "Page size" as const;

const LIST_LIMIT_MIN = 1;
const LIST_LIMIT_MAX = 200;
const EVENTS_LIMIT_MAX = 500;
const BILLING_LIMIT_MAX = 100;

/** A whole number the server will accept, refused here rather than over the wire. */
const boundedInt = (name: string, min: number, max: number) =>
  Flag.Int(name).pipe(
    Flag.filter(
      (n: number) => n >= min && n <= max,
      () => `must be between ${min} and ${max}`,
    ),
  );

/**
 * An identifier flag, checked before any request goes out.
 *
 * Canonical lowercase uuidv7, matching `afd_core::id` on the server — an
 * uppercase alias is the same row in Postgres and a different key in
 * Dragonfly, so accepting one here would only move the refusal later.
 */
const idFlag = (name: string, description: string) =>
  Flag.String(name).pipe(
    Flag.withDescription(description),
    Flag.filter((value: string) => isValidId(value), () => NOT_A_UUIDV7),
    Flag.optional,
  );

const textFlag = (name: string, description: string) =>
  Flag.String(name).pipe(Flag.withDescription(description), Flag.optional);

export const workspaceIdFlag = idFlag("workspace-id", WORKSPACE_ID_DESC);
export const workspaceFlag = idFlag("workspace", WORKSPACE_ID_DESC);
export const fleetFlag = idFlag("fleet", FLEET_ID_DESC);

export const cursorFlag = textFlag("cursor", NEXT_CURSOR);
export const startingAfterFlag = textFlag("starting-after", NEXT_CURSOR);

export const listLimitFlag = boundedInt(FLAG.limit, LIST_LIMIT_MIN, LIST_LIMIT_MAX).pipe(
  Flag.withDescription(PAGE_SIZE),
  Flag.optional,
);
export const eventsLimitFlag = boundedInt(FLAG.limit, LIST_LIMIT_MIN, EVENTS_LIMIT_MAX).pipe(
  Flag.withDescription(PAGE_SIZE),
  Flag.optional,
);
export const memoryLimitFlag = boundedInt(FLAG.limit, LIST_LIMIT_MIN, MAX_RECALL_LIMIT).pipe(
  Flag.withDescription("Max entries to return"),
  Flag.optional,
);
export const billingLimitFlag = boundedInt(FLAG.limit, LIST_LIMIT_MIN, BILLING_LIMIT_MAX).pipe(
  Flag.withDescription("Number of recent events to show"),
  Flag.optional,
);

// The accepted set is whatever `GET /v1/models` serves on the server the
// caller is pointed at, so it cannot be a literal union here. The handler
// checks it against the live catalogue — the same bytes the dashboard's
// provider dropdown is built from.
export const providerFlag = textFlag(
  FLAG.provider,
  `Provider id from \`agentsfleet models\` (use '${OPENAI_COMPATIBLE_PROVIDER}' with --base-url for an endpoint the catalogue does not carry)`,
);

/**
 * The client-side half of the custom-endpoint check.
 *
 * Rejecting a non-https URL at parse time means no request is made at all.
 * The full check — loopback, private ranges, cloud metadata hosts — stays on
 * the server in `base_url_guard`, because only it knows what its own network
 * looks like.
 */
export const baseUrlFlag = Flag.String("base-url").pipe(
  Flag.withDescription(
    "Custom endpoint base URL (https; required for a custom-endpoint provider)",
  ),
  Flag.filter(
    (raw: string) => {
      const trimmed = raw.trim();
      if (!trimmed.startsWith(HTTPS_SCHEME_PREFIX)) return false;
      try {
        return new URL(trimmed).protocol === "https:";
      } catch {
        return false;
      }
    },
    () => "must be an https URL",
  ),
  Flag.optional,
);

export const apiKeyFlag = textFlag(
  "api-key",
  "Provider API key (required with a named --provider, optional for a keyless custom endpoint)",
);
export const modelFlag = textFlag(FLAG.model, "Default model identifier (required with --provider)");
export const dataFlag = textFlag(FLAG.data, "Secret JSON object, or @- to read stdin");
export const dataReplacementFlag = textFlag(
  FLAG.data,
  "Replacement JSON object, or @- to read stdin",
);

export const nameFlag = textFlag(
  FLAG.name,
  "Override the fleet name (install the same bundle more than once)",
);
export const libraryFlag = textFlag("library", "Library id from `agentsfleet library`");
export const fromPathFlag = textFlag(FLAG.from, "Skill bundle path");
export const githubFlag = textFlag(
  "github",
  "Public GitHub repository carrying SKILL.md at its root",
);
export const fromBundleFlag = textFlag(FLAG.from, "Local bundle directory to upload");
export const templateFlag = textFlag("template", "First-party template id");
export const refFlag = textFlag("ref", "Branch, tag, or commit (--github only)");

export const categoryFlag = textFlag("category", "Filter by category");
export const actorFlag = textFlag("actor", "Filter by actor glob");
export const sinceFlag = textFlag("since", "RFC 3339 or duration (e.g. 2h)");

export const cronFlag = Flag.String(FLAG.cron).pipe(Flag.withDescription(DESC_CRON));
export const messageFlag = Flag.String(FLAG.message).pipe(Flag.withDescription(DESC_MESSAGE));
export const cronOptionalFlag = textFlag(FLAG.cron, DESC_CRON);
export const messageOptionalFlag = textFlag(FLAG.message, DESC_MESSAGE);
export const timezoneFlag = textFlag(FLAG.timezone, "IANA timezone");
export const timezoneDefaultFlag = textFlag(FLAG.timezone, "IANA timezone (default: UTC)");
export const scheduleStatusFlag = textFlag("status", "active or paused");

export const sortFlag = Flag.Literals("sort", API_KEY_SORTS).pipe(
  Flag.withDescription("Sort order"),
  Flag.optional,
);

export const keyNameFlag = textFlag(FLAG.name, "Human-readable key name");
export const descriptionFlag = textFlag("description", "Optional description");
export const secretNameFlag = textFlag("secret", "Named secret from the workspace vault");
export const modelOverrideFlag = textFlag(FLAG.model, "Override the default model identifier");

export const tokenNameFlag = textFlag(
  "token-name",
  "Label for this session, shown on the approval page and in `auth status` (default: platform family)",
);
export const forceFlag = Flag.Boolean("force").pipe(
  Flag.withDescription("Skip the existing-credential prompt and overwrite"),
);

// Kept so the flag still parses and the handler can refuse it by name. Logout
// already revokes what it can; a person who reads "all sessions" and believes
// a lost laptop is signed out has been told something untrue, so the flag
// exits with a validation error rather than quietly doing less than it says.
export const logoutAllFlag = Flag.Boolean("all").pipe(
  Flag.withDescription(
    "rejected — logout already revokes what it can; passing this flag exits with a validation error",
  ),
);

export const installLibraryDescription =
  `Install a Fleet from a library entry (--library ${LIBRARY_ID_PLACEHOLDER})` as const;

// ── Positional arguments ────────────────────────────────────────────

const idArgument = (name: string, description: string) =>
  Argument.String(name).pipe(
    Argument.withDescription(description),
    Argument.filter((value: string) => isValidId(value), () => NOT_A_UUIDV7),
  );

export const fleetIdArgument = idArgument("fleet_id", FLEET_ID_DESC);
export const fleetIdOptionalArgument = fleetIdArgument.pipe(Argument.optional);
export const workspaceIdArgument = idArgument("workspace_id", WORKSPACE_ID_DESC);
export const workspaceIdOptionalArgument = workspaceIdArgument.pipe(Argument.optional);
export const gateIdArgument = idArgument("gate_id", "Approval gate ID");
export const apiKeyIdArgument = idArgument("api_key_id", "API key ID");
export const grantIdArgument = idArgument("grant_id", "Grant ID");
export const scheduleIdArgument = idArgument("schedule_id", "Schedule ID");

export const workspaceNameArgument = Argument.String(FLAG.name).pipe(
  Argument.withDescription("Workspace name"),
);
export const secretNameArgument = Argument.String(FLAG.name).pipe(
  Argument.withDescription("Secret name"),
);
export const providerArgument = Argument.String(FLAG.provider).pipe(
  Argument.withDescription("Connector provider"),
);
export const queryArgument = Argument.String("query").pipe(
  Argument.withDescription("Substring to search for"),
);
export const messageArgument = Argument.String(FLAG.message).pipe(
  Argument.withDescription("Message to send"),
  Argument.optional,
);
