"use client";

import { useState, useTransition } from "react";
import { ServerIcon } from "lucide-react";
import { Alert, Button, EmptyState } from "@agentsfleet/design-system";
import type { RunnerListItem } from "@/lib/api/runners";
import { presentErrorString } from "@/lib/errors";
import { listRunnersAction } from "../actions";
import RunnerTile from "./RunnerTile";

// The wall over the runner list — the Fleet wall grammar applied to hosts.
// Every runner gets a card whose whole face links to its detail page; zero
// runners renders the empty state, never a bare grid.

const RUNNERS_EMPTY_TITLE = "No runners enrolled";
const RUNNERS_EMPTY_DESCRIPTION = "Enroll a host to run fleets and it appears here.";
const LOAD_MORE_LABEL = "Load more";
const LOADING_LABEL = "Loading…";
const LOAD_MORE_ERROR_ACTION = "load more runners";

export default function RunnerWall({
  initialRunners,
  initialCursor,
}: {
  initialRunners: RunnerListItem[];
  initialCursor: string | null;
}) {
  const [runners, setRunners] = useState<RunnerListItem[]>(initialRunners);
  const [cursor, setCursor] = useState<string | null>(initialCursor);
  const [error, setError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();

  function loadMore(next: string) {
    setError(null);
    startTransition(async () => {
      const result = await listRunnersAction({ starting_after: next });
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
      setRunners((prev) => [...prev, ...result.data.items]);
      setCursor(result.data.next_cursor);
    });
  }

  if (runners.length === 0) {
    return (
      <EmptyState
        icon={<ServerIcon size={28} />}
        title={RUNNERS_EMPTY_TITLE}
        description={RUNNERS_EMPTY_DESCRIPTION}
      />
    );
  }

  return (
    <div>
      <div className="grid grid-cols-1 gap-lg sm:grid-cols-2 lg:grid-cols-3">
        {runners.map((runner) => (
          <RunnerTile key={runner.id} runner={runner} />
        ))}
      </div>

      {error ? (
        <Alert variant="destructive" className="mt-lg">{error}</Alert>
      ) : null}

      {cursor ? (
        <div className="mt-xl flex justify-center">
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
      ) : null}
    </div>
  );
}
