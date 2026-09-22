"use client";

import { useOptimistic, useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import {
  Button,
  ConfirmDialog,
  DataTable,
  EmptyState,
  Time,
  type DataTableColumn,
} from "@agentsfleet/design-system";
import { LibraryIcon } from "lucide-react";
import { listLibraryEntriesAction, removeLibraryEntryAction } from "../actions";
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
  LOAD_MORE_LABEL,
  LOAD_MORE_ERROR_ACTION,
  LOADING_LABEL,
  REMOVE_CONFIRM_LABEL,
  REMOVE_DIALOG_BODY,
  REMOVE_DIALOG_TITLE,
} from "../copy";

type Props = {
  workspaceId: string;
  entries: WorkspaceLibraryEntry[];
  /** Where the next page resumes, or `null` when the first page is all of it. */
  initialCursor: string | null;
};

const REMOVE_ACTION_DESCRIPTION = "remove the library entry";

/** `github:acme/reviewer` — where the bytes came from, which is what tells two
 *  near-identical onboardings apart before anything else on the row does. */
function provenance(entry: WorkspaceLibraryEntry): string {
  return entry.source_kind ? `${entry.source_kind}:${entry.source_ref}` : entry.source_ref;
}

/** The onboarding instant, rendered by the design system.
 *
 *  The column answers "which of these is the one I added this morning", which
 *  a relative label answers better than either a date or a millisecond count.
 *  `Time` owns the `<time datetime>` semantic, the locale pin and the
 *  hydration guard — `tests/timestamp-standard.test.ts` is the grep that keeps
 *  a hand-rolled formatter from reappearing here. */
function onboardedOn(entry: WorkspaceLibraryEntry) {
  return (
    <Time value={new Date(entry.created_at)} format="relative" className="tabular-nums" />
  );
}

function buildColumns({
  onRemove,
}: {
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
      // Never disabled. Opening the question sends nothing — it sets local
      // state and nothing else — so there is no request to guard against, and
      // `pending` here is shared with Load more: appending a page used to
      // shut this button for as long as the fetch ran. A click landing in
      // that window is not queued or replayed, it is dropped, so a person
      // clicked Remove, saw nothing happen, and had to discover that a second
      // click works. The destructive step is the dialog's own confirm, which
      // is where the guard belongs and where it still is.
      cell: (entry) => (
        <Button variant="ghost" size="sm" onClick={() => onRemove(entry)}>
          {REMOVE_CONFIRM_LABEL}
        </Button>
      ),
    },
  ];
}

export default function WorkspaceLibraryList({ workspaceId, entries, initialCursor }: Props) {
  const router = useRouter();
  const [pending, startTransition] = useTransition();
  const [target, setTarget] = useState<WorkspaceLibraryEntry | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Pages after the first. The server renders page one; these accumulate as
  // the operator asks for more, so the table shows everything fetched rather
  // than the first hundred and a silence.
  const [appended, setAppended] = useState<WorkspaceLibraryEntry[]>([]);
  const [cursor, setCursor] = useState<string | null>(initialCursor);
  const all = [...entries, ...appended];
  // The row leaves the table the moment the operator confirms; the server is
  // told inside the same transition. A rejected removal ends that transition
  // and React restores the row from the server-rendered list on its own — the
  // same shape the secrets list uses, so nothing here has to put it back.
  const [visible, hideEntry] = useOptimistic(
    all,
    (current: WorkspaceLibraryEntry[], removedId: string) =>
      current.filter((entry) => entry.id !== removedId),
  );

  // A removal invalidates every page after the first. The server re-renders
  // page one, but these are client state and would survive it: the collection
  // is keyset-paged, so taking a row out shifts the rest up across the page
  // boundary, and the refreshed first page then re-contains a row `appended`
  // still holds. That renders the row twice under one key, and leaves Load
  // more resuming from a position that no longer exists. So the pages go back
  // to what the server just said, and the cursor with them.
  function resetPaging() {
    setAppended([]);
    setCursor(initialCursor);
  }

  // Mirrors the runner wall: append the page, follow its cursor, and keep the
  // rows already shown when a page fails.
  function loadMore(next: string) {
    setError(null);
    startTransition(async () => {
      const result = await listLibraryEntriesAction(workspaceId, next);
      if (!result.ok) {
        setError(
          presentErrorString({
            errorCode: result.errorCode,
            message: result.error,
            action: LOAD_MORE_ERROR_ACTION,
          }),
        );
        return;
      }
      setAppended((prev) => [...prev, ...result.data.items]);
      setCursor(result.data.next_cursor);
    });
  }

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
            // An outcome we cannot read may have removed the row, so the
            // re-read is server truth — and the pages after the first are
            // invalidated by it exactly as they are on success.
            if (!isDefiniteRefusal(result.status)) {
              resetPaging();
              router.refresh();
            }
            return;
          }
          setTarget(null);
          resetPaging();
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
      {cursor === null ? null : (
        <div className="flex justify-center">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => loadMore(cursor)}
            disabled={pending}
            aria-busy={pending}
          >
            {pending ? LOADING_LABEL : LOAD_MORE_LABEL}
          </Button>
        </div>
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
