// Copy and routing constants for the platform fleet-library surface, kept in a
// neutral module so the server page, the table, and the dialogs share one
// spelling.

export const FLEET_LIBRARIES_TITLE = "Fleet libraries";

export const FLEET_LIBRARIES_DESCRIPTION =
  "Add a fleet from its GitHub repository, write what its install gate says, then publish it. Only a published fleet reaches a workspace.";

export const ADMIN_FLEET_LIBRARIES_PATH = "/admin/fleet-libraries";

// Where a caller without `platform-library:write` is sent. Mirrors the notice
// shape the models surface uses for the same class of redirect.
export const NOT_PLATFORM_ADMIN = "/settings?notice=fleet-libraries-platform-admin-only";

// ── The operator's verbs ─────────────────────────────────────────────────────
//
// "Onboard" is the internal name of the write. It is not what an operator is
// doing, so it appears in no label: they add a fleet, fetch its bundle, publish
// it, or take it back.

export const ADD_FLEET = "Add fleet";
export const FETCH_BUNDLE = "Fetch bundle";
export const FETCH_UPDATE = "Fetch update";
export const PUBLISH = "Publish";
export const UNPUBLISH = "Unpublish";
export const EDIT = "Edit";
export const DELETE = "Delete";

export const ADD_ACTION = "add the fleet";
export const PATCH_ACTION = "update the fleet";
export const DELETE_ACTION = "delete the fleet";
export const CATALOG_READ_ACTION = "load the fleet catalog";

export const ADD_TOOLTIP =
  "Fetch a fleet's bundle from GitHub. It lands as a draft — no workspace sees it until you publish.";
// IconAction folds the tooltip into `label` — the accessible name IS the tooltip,
// so a second string would be a second spelling of the same thing. The verbs above
// are those labels; the explanatory copy lives on the status badges and in the
// destructive confirm, where an operator is actually deciding something.

// ── Status ───────────────────────────────────────────────────────────────────

export const STATUS_LABEL_PUBLISHED = "Published";
export const STATUS_LABEL_DRAFT = "Draft";
export const STATUS_LABEL_NO_BUNDLE = "No bundle";

export const STATUS_HELP_PUBLISHED = "Live in every workspace gallery.";
export const STATUS_HELP_DRAFT = "Bundle stored. No workspace can see it.";
export const STATUS_HELP_NO_BUNDLE = "No bundle has been fetched yet.";

// ── Table ────────────────────────────────────────────────────────────────────

export const COLUMN_FLEET = "Fleet";
export const COLUMN_REPOSITORY = "Repository";
export const COLUMN_STATUS = "Status";
export const COLUMN_BUNDLE = "Bundle";
export const COLUMN_ACTIONS = "Actions";

export const EMPTY_TITLE = "No fleets in the catalog";
export const EMPTY_DESCRIPTION =
  "Add a fleet from its GitHub repository. Nothing is published until you say so.";

// The hash is long and an operator only ever compares it — enough to tell two
// bundles apart, not enough to dominate the row.
export const HASH_PREVIEW_LENGTH = 12;

// ── Dialogs ──────────────────────────────────────────────────────────────────

export const DELETE_CONFIRM_TITLE = "Delete this fleet?";
export const DELETE_CONFIRM_BODY =
  "It leaves the catalog. Workspaces already running it are unaffected — their install holds its own copy of the bundle.";

export const REPLACE_CONFIRM =
  "That name already belongs to a different repository. Replacing it swaps the bundle every workspace installs.";
export const REPLACE_ACTION = "Replace anyway";

export const EDIT_TITLE = "Edit install-gate copy";
export const EDIT_DESCRIPTION =
  "What a user reads when this fleet asks for their credentials. A bundle refetch never overwrites it.";
export const EDIT_DESCRIPTION_LABEL = "Description";
export const EDIT_REASON_LABEL = "Why this fleet needs each credential";

// The source shape (accepted form, example repository, authoring docs) is shared
// with the workspace onboarding dialog — see lib/fleet-library-source.ts. It is
// deliberately NOT re-spelled here: both surfaces feed the same importer, so a
// tightening must reach both.
export {
  SOURCE_REF_PATTERN,
  SAMPLE_LIBRARY_REPO,
  LIBRARY_AUTHORING_DOC_URL,
} from "@/lib/fleet-library-source";
