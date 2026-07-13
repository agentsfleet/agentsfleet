import {
  CATALOG_STATUS_BROKEN,
  CATALOG_STATUS_DRAFT,
  CATALOG_STATUS_NO_BUNDLE,
  CATALOG_STATUS_PUBLISHED,
  catalogStatus,
  type CatalogStatus,
  type PlatformCatalogEntry,
} from "@/lib/types";
import {
  STATUS_HELP_BROKEN,
  STATUS_HELP_DRAFT,
  STATUS_HELP_NO_BUNDLE,
  STATUS_HELP_PUBLISHED,
  STATUS_LABEL_BROKEN,
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
  tone: "green" | "amber" | "default" | "destructive";
};

// Exhaustive by construction: a CatalogStatus that forgets its view fails the
// type check, so a new state cannot reach the table unpresented.
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
  // A fault, not a lifecycle step — destructive, not the draft amber. This row
  // needs an operator; a draft is merely awaiting one.
  [CATALOG_STATUS_BROKEN]: {
    label: STATUS_LABEL_BROKEN,
    help: STATUS_HELP_BROKEN,
    tone: "destructive",
  },
};

export function statusView(entry: PlatformCatalogEntry): StatusView {
  return VIEWS[catalogStatus(entry)];
}

// What an operator may do to a row, derived from its state. No affordance is
// rendered for a state it cannot serve — a published fleet has no Delete at all,
// rather than a disabled one, because a disabled button is a promise. The route
// enforces the same guards (UZ-CATALOG-002 / UZ-CATALOG-003); this is only the
// honest surface.
//
// Publish and Delete key off the BUNDLE and the VISIBILITY respectively, not off
// the status name. A broken row (public, no bundle) is still public, so the
// server refuses to delete it exactly as it refuses for a published one —
// keying Delete on `status !== published` would offer a button the route denies.
export type RowActions = {
  /** A row with no bundle cannot be published — it has nothing to serve. */
  canPublish: boolean;
  /** Withdrawing is how a broken row is made honest, so both public states offer it. */
  canUnpublish: boolean;
  /** Withdraw before deleting: never take a live fleet from the tenants using it. */
  canDelete: boolean;
};

export function rowActions(entry: PlatformCatalogEntry): RowActions {
  const status = catalogStatus(entry);
  const isPublic = status === CATALOG_STATUS_PUBLISHED || status === CATALOG_STATUS_BROKEN;
  return {
    canPublish: status === CATALOG_STATUS_DRAFT,
    canUnpublish: isPublic,
    canDelete: !isPublic,
  };
}
