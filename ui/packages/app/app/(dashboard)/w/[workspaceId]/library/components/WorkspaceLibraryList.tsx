"use client";

import { useOptimistic, useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import {
  Button,
  ConfirmDialog,
  DataTable,
  EmptyState,
  type DataTableColumn,
} from "@agentsfleet/design-system";
import { LibraryIcon } from "lucide-react";
import { removeLibraryEntryAction } from "../actions";
import type { WorkspaceLibraryEntry } from "@/lib/api/library-types";
import { isDefiniteRefusal } from "@/lib/api/errors";
import { presentErrorString } from "@/lib/errors";
import {
  COLUMN_ACTIONS,
  COLUMN_NAME,
  COLUMN_ONBOARDED,
  COLUMN_SOURCE,
  LIBRARY_EMPTY_BODY,
  LIBRARY_EMPTY_TITLE,
  LIBRARY_SECTION_LABEL,
  REMOVE_CONFIRM_LABEL,
  REMOVE_DIALOG_BODY,
  REMOVE_DIALOG_TITLE,
} from "../copy";

type Props = {
  workspaceId: string;
  entries: WorkspaceLibraryEntry[];
};

const REMOVE_ACTION_DESCRIPTION = "remove the library entry";

/** `github:acme/reviewer` — where the bytes came from, which is what tells two
 *  near-identical onboardings apart before anything else on the row does. */
function provenance(entry: WorkspaceLibraryEntry): string {
  return entry.source_kind ? `${entry.source_kind}:${entry.source_ref}` : entry.source_ref;
}

/** The onboarding day. The column answers "which of these is the one I added
 *  this morning", which a date answers and a millisecond count does not. */
function onboardedOn(entry: WorkspaceLibraryEntry): string {
  return new Date(entry.created_at).toLocaleDateString();
}

function buildColumns({
  pending,
  onRemove,
}: {
  pending: boolean;
  onRemove: (entry: WorkspaceLibraryEntry) => void;
}): DataTableColumn<WorkspaceLibraryEntry>[] {
  return [
    {
      key: "name",
      header: COLUMN_NAME,
      sortValue: (entry) => entry.name,
      cell: (entry) => entry.name,
    },
    {
      key: "source",
      header: COLUMN_SOURCE,
      hideOnMobile: true,
      sortValue: provenance,
      cell: provenance,
    },
    {
      key: "created_at",
      header: COLUMN_ONBOARDED,
      hideOnMobile: true,
      sortValue: (entry) => entry.created_at,
      cell: onboardedOn,
    },
    {
      key: "actions",
      header: COLUMN_ACTIONS,
      numeric: true,
      cell: (entry) => (
        <Button
          variant="ghost"
          size="sm"
          disabled={pending}
          onClick={() => onRemove(entry)}
        >
          {REMOVE_CONFIRM_LABEL}
        </Button>
      ),
    },
  ];
}

export default function WorkspaceLibraryList({ workspaceId, entries }: Props) {
  const router = useRouter();
  const [pending, startTransition] = useTransition();
  const [target, setTarget] = useState<WorkspaceLibraryEntry | null>(null);
  const [error, setError] = useState<string | null>(null);
  // The row leaves the table the moment the operator confirms; the server is
  // told inside the same transition. A rejected removal ends that transition
  // and React restores the row from the server-rendered list on its own — the
  // same shape the secrets list uses, so nothing here has to put it back.
  const [visible, hideEntry] = useOptimistic(
    entries,
    (current: WorkspaceLibraryEntry[], removedId: string) =>
      current.filter((entry) => entry.id !== removedId),
  );

  // Resolves when the transition settles, so the dialog holds its buttons
  // disabled until then. A confirm that returned at once would leave the
  // dialog live, and a second click would send a second removal.
  function onConfirmRemove(entry: WorkspaceLibraryEntry): Promise<void> {
    setError(null);
    return new Promise<void>((resolve) => {
      startTransition(async () => {
        try {
          hideEntry(entry.id);
          const result = await removeLibraryEntryAction(workspaceId, entry.id);
          if (!result.ok) {
            setError(
              presentErrorString({
                errorCode: result.errorCode,
                message: result.error,
                action: REMOVE_ACTION_DESCRIPTION,
              }),
            );
            // A refusal the server made is a no-op: the row is back when the
            // transition ends. A failure whose outcome is unknown — a timeout,
            // a transport fault — may have removed the row anyway, and the
            // re-read makes its return or absence server truth.
            if (!isDefiniteRefusal(result.status)) router.refresh();
            return;
          }
          setTarget(null);
          router.refresh();
        } finally {
          resolve();
        }
      });
    });
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3">
      {/* "Nothing onboarded" is the server's claim to make: the optimistic
          hide of the last row keeps the table shell until the removal is
          answered, so the status region never announces an emptiness the
          server may retract. */}
      {entries.length === 0 ? (
        <EmptyState
          icon={<LibraryIcon size={28} />}
          title={LIBRARY_EMPTY_TITLE}
          description={LIBRARY_EMPTY_BODY}
        />
      ) : (
        <DataTable
          className="flex min-h-0 flex-1 flex-col"
          columns={buildColumns({
            pending,
            onRemove: (entry) => {
              setError(null);
              setTarget(entry);
            },
          })}
          rows={visible}
          rowKey={(entry) => entry.id}
          caption={LIBRARY_SECTION_LABEL}
          viewportClassName="min-h-0 flex-1 max-h-none"
        />
      )}
      <ConfirmDialog
        open={target !== null}
        onOpenChange={() => setTarget(null)}
        title={REMOVE_DIALOG_TITLE(target?.name ?? "")}
        description={REMOVE_DIALOG_BODY}
        confirmLabel={REMOVE_CONFIRM_LABEL}
        intent="destructive"
        errorMessage={error}
        onConfirm={target ? () => onConfirmRemove(target) : undefined}
      />
    </div>
  );
}
