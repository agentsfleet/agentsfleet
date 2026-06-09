"use client";

import {
  Badge,
  Button,
  ConfirmDialog,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@usezombie/design-system";
import type {
  RunnerAdminAction,
  RunnerListItem,
  RunnerEventItem,
  RunnerEventsResponse,
} from "@/lib/api/runners";

const RUNNER_ACTIVITY_TITLE = "Runner activity";
const CONFIRM_LABEL = "Confirm";
const EMPTY_METADATA = "{}";

export type RunnerActionConfirmTarget = {
  runner: RunnerListItem;
  action: RunnerAdminAction;
  label: string;
  title: string;
  description: string;
  intent: "default" | "destructive";
  errorAction: string;
} | null;

export function RunnerActionConfirm({
  target,
  error,
  onOpenChange,
  onConfirm,
}: {
  target: RunnerActionConfirmTarget;
  error: string | null;
  onOpenChange: (open: boolean) => void;
  onConfirm: (target: NonNullable<RunnerActionConfirmTarget>) => void;
}) {
  return (
    <ConfirmDialog
      open={target !== null}
      onOpenChange={onOpenChange}
      title={target?.title ?? ""}
      description={target?.description ?? ""}
      confirmLabel={target?.label ?? CONFIRM_LABEL}
      intent={target?.intent ?? "default"}
      errorMessage={error}
      onConfirm={target ? () => onConfirm(target) : undefined}
    />
  );
}

export function RunnerActivityDialog({
  runner,
  data,
  error,
  pending,
  onOpenChange,
  onPage,
}: {
  runner: RunnerListItem;
  data: RunnerEventsResponse | null;
  error: string | null;
  pending: boolean;
  onOpenChange: (open: boolean) => void;
  onPage: (page: number) => void;
}) {
  const lastPage = data ? Math.max(1, Math.ceil(data.total / data.page_size)) : 1;
  return (
    <Dialog open onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>{RUNNER_ACTIVITY_TITLE}</DialogTitle>
          <DialogDescription>{runner.host_id}</DialogDescription>
        </DialogHeader>
        <div className="flex flex-wrap gap-2">
          <Badge variant="default">{runner.admin_state}</Badge>
          <Badge variant="default">{runner.liveness}</Badge>
          {runner.labels.map((label) => <Badge key={label} variant="default">{label}</Badge>)}
        </div>
        {error ? <p className="text-sm text-destructive">{error}</p> : null}
        {data?.items.length === 0 ? <p className="text-sm text-muted-foreground">No activity yet.</p> : null}
        <ul className="max-h-96 space-y-3 overflow-y-auto pr-1" aria-label={RUNNER_ACTIVITY_TITLE}>
          {data?.items.map((event) => <ActivityRow key={event.id} event={event} />)}
        </ul>
        {data && lastPage > 1 ? (
          <div className="flex items-center justify-between text-sm text-muted-foreground">
            <span>
              Page {data.page} of {lastPage} · {data.total} events
            </span>
            <div className="flex gap-2">
              <Button type="button" variant="ghost" size="sm" disabled={pending || data.page <= 1} onClick={() => onPage(data.page - 1)}>
                Previous
              </Button>
              <Button type="button" variant="ghost" size="sm" disabled={pending || data.page >= lastPage} onClick={() => onPage(data.page + 1)}>
                Next
              </Button>
            </div>
          </div>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}

function ActivityRow({ event }: { event: RunnerEventItem }) {
  const occurredAt = new Date(event.occurred_at);
  return (
    <li className="rounded-md border p-3">
      <div className="flex flex-wrap items-center gap-2">
        <Badge variant="cyan">{event.event_type}</Badge>
        <time className="font-mono text-xs tabular-nums text-muted-foreground" dateTime={occurredAt.toISOString()}>
          {occurredAt.toLocaleString()}
        </time>
      </div>
      <p className="mt-2 break-all font-mono text-xs text-muted-foreground">{formatMetadata(event.metadata)}</p>
    </li>
  );
}

function formatMetadata(metadata: unknown): string {
  try {
    return JSON.stringify(metadata ?? {});
  } catch {
    return EMPTY_METADATA;
  }
}
