import {
  CATALOG_STATUS_DRAFT,
  CATALOG_STATUS_NO_BUNDLE,
  CATALOG_STATUS_PUBLISHED,
  catalogStatus,
  type CatalogStatus,
  type PlatformCatalogEntry,
} from "@/lib/types";
import {
  STATUS_HELP_DRAFT,
  STATUS_HELP_NO_BUNDLE,
  STATUS_HELP_PUBLISHED,
  STATUS_LABEL_DRAFT,
  STATUS_LABEL_NO_BUNDLE,
  STATUS_LABEL_PUBLISHED,
} from "../library-copy";

// The presentation of a row's state, derived in one place from `catalogStatus`.
// The status itself is never a wire field — two sources for one fact is how a
// table starts lying.

type StatusView = {
  label: string;
  help: string;
  tone: "green" | "amber" | "default";
};

const VIEWS: Record<CatalogStatus, StatusView> = {
  [CATALOG_STATUS_PUBLISHED]: {
    label: STATUS_LABEL_PUBLISHED,
    help: STATUS_HELP_PUBLISHED,
    tone: "green",
  },
  [CATALOG_STATUS_DRAFT]: {
    label: STATUS_LABEL_DRAFT,
    help: STATUS_HELP_DRAFT,
    tone: "amber",
  },
  [CATALOG_STATUS_NO_BUNDLE]: {
    label: STATUS_LABEL_NO_BUNDLE,
    help: STATUS_HELP_NO_BUNDLE,
    tone: "default",
  },
};

export function statusView(entry: PlatformCatalogEntry): StatusView {
  return VIEWS[catalogStatus(entry)];
}

// What an operator may do to a row, derived from its state. No affordance is
// rendered for a state it cannot serve — a published fleet has no Delete at all,
// rather than a disabled one, because a disabled button is a promise. The route
// enforces the same guard (UZ-CATALOG-003); this is only the honest surface.
export type RowActions = {
  /** A row with no bundle cannot be published — it has nothing to serve. */
  canPublish: boolean;
  canUnpublish: boolean;
  /** Withdraw before deleting: never take a live fleet from the tenants using it. */
  canDelete: boolean;
};

export function rowActions(entry: PlatformCatalogEntry): RowActions {
  const status = catalogStatus(entry);
  return {
    canPublish: status === CATALOG_STATUS_DRAFT,
    canUnpublish: status === CATALOG_STATUS_PUBLISHED,
    canDelete: status !== CATALOG_STATUS_PUBLISHED,
  };
}
