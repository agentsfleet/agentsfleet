"use client";

import type { ReactNode } from "react";
import { Badge, CopyButton, cn } from "@agentsfleet/design-system";
import { LockIcon } from "lucide-react";
import { providerLabel } from "@/lib/api/model-library-types";
import { modelIdCopyLabel, modelLabel } from "@/lib/models/display";

// The provider and model cells of the two model tables — the workspace
// registry and the platform library — rendered from one place, so a provider
// is spelled the same way, in the same badge, on both, and a model id is
// reached the same way (name reads, id on hover, copy button) on both. Pure
// props → markup; each table keeps its own row shapes and actions.

/** The one statement a lock beside a model makes: a tenant admin cannot edit this row. */
export const PLATFORM_MANAGED_LABEL = "Managed by a platform admin";
/** A row the server sent without a provider id. */
const PROVIDER_UNKNOWN = "Unknown";
const LOCK_ICON_SIZE = 12;

/** Muted text: a value that is absent or that this viewer cannot change. */
const MUTED = "text-muted-foreground";

/**
 * The vendor's own spelling of a provider id, in the badge both tables share.
 * A row with no provider reads as plain muted text, so an absent value never
 * looks like a vendor named Unknown.
 */
export function ProviderBadge({ provider }: { provider: string | undefined }) {
  if (!provider) return <span className={MUTED}>{PROVIDER_UNKNOWN}</span>;
  // The badge primitive uppercases by default, which is right for a status
  // and wrong for a name: "ATLAS-CLOUD" reads as a slug, "Atlas Cloud" reads
  // as a vendor. Same override the approvals and lease filter badges carry.
  // The neutral variant, because a name is not a status: colour stays for
  // the badges that say something, such as a row's Default.
  return (
    <Badge className="normal-case">
      {providerLabel(provider)}
    </Badge>
  );
}

/** The lock a platform-managed model carries; the label is its whole meaning. */
export function PlatformManagedMark() {
  return <LockIcon size={LOCK_ICON_SIZE} className={cn("shrink-0", MUTED)} aria-label={PLATFORM_MANAGED_LABEL} />;
}

/**
 * A model as both tables name it: the marketed name reads, the id a fleet
 * config and a provider dashboard need is one hover away, and the copy
 * button reaches it on touch or a keyboard.
 */
export function ModelName({
  modelId,
  managedByPlatform = false,
  trimmed = false,
  trailing,
}: {
  modelId: string;
  /** A fixed name width, so what follows the name lines up down the table. */
  trimmed?: boolean;
  /** Marks the row as a platform admin's, which a tenant admin cannot edit. */
  managedByPlatform?: boolean;
  /** What follows the name on this table, such as the library's Default badge. */
  trailing?: ReactNode;
}) {
  return (
    <span className="flex min-w-0 items-center gap-2">
      {managedByPlatform ? <PlatformManagedMark /> : null}
      <span className="flex min-w-0 items-center gap-1">
        {/* A row a tenant admin cannot edit reads muted, as it did before
            the two tables shared this cell. */}
        <span
          className={cn("block min-w-0 truncate", trimmed && "sm:min-w-trim sm:max-w-trim", managedByPlatform && MUTED)}
          title={modelId}
        >
          {modelLabel(modelId)}
        </span>
        <CopyButton value={modelId} label={modelIdCopyLabel(modelId)} />
      </span>
      {trailing}
    </span>
  );
}
