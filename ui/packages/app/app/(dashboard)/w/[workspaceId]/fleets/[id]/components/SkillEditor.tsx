"use client";

import { useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import { PencilIcon } from "lucide-react";
import {
  Alert,
  Badge,
  Button,
  Card,
  ConfirmDialog,
  CopyButton,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  Textarea,
  cn,
} from "@agentsfleet/design-system";
import { getFleetDetailAction, saveFleetSourceAction } from "../../actions";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { EVENTS } from "@/lib/analytics/events";
import { presentErrorString } from "@/lib/errors";
import {
  CANCEL_EDIT_LABEL,
  DIFF_ADDED_PREFIX,
  DIFF_NO_CHANGES,
  DIFF_PANEL_TITLE,
  DIFF_REMOVED_PREFIX,
  EDIT_SOURCE_LABEL,
  OUTCOME,
  SAVE_CONFIRM_LABEL,
  SAVE_DIALOG_TITLE,
  SAVE_NEXT_WAKE_NOTICE,
  SAVE_SOURCE_LABEL,
  SAVE_STALE_RELOADED_NOTICE,
  SKILL_DOC_LABEL,
  SOURCE_FIELD,
  SOURCE_PANEL_TITLE,
  TRIGGER_DOC_EMPTY,
  TRIGGER_DOC_LABEL,
  type SourceField,
} from "./console-copy";

// HTTP 412 Precondition Failed — the source changed under an open editor, so the
// save was refused rather than overwriting (M131 §4). Named so the branch reads
// as intent, not a magic number.
const PRECONDITION_FAILED = 412;

type Props = {
  workspaceId: string;
  fleetId: string;
  sourceMarkdown: string;
  triggerMarkdown: string | null;
  etag: string | null;
};

// The two editable documents, keyed by the analytics `field` value. `body` maps
// each to the PATCH field name so a save sends only the document that changed.
type DocState = Record<SourceField, string>;

const PATCH_FIELD: Record<SourceField, "source_markdown" | "trigger_markdown"> = {
  [SOURCE_FIELD.skill]: "source_markdown",
  [SOURCE_FIELD.trigger]: "trigger_markdown",
};

export default function SkillEditor({
  workspaceId,
  fleetId,
  sourceMarkdown,
  triggerMarkdown,
  etag: initialEtag,
}: Props) {
  const router = useRouter();
  const [base, setBase] = useState<DocState>({
    [SOURCE_FIELD.skill]: sourceMarkdown,
    [SOURCE_FIELD.trigger]: triggerMarkdown ?? "",
  });
  const [draft, setDraft] = useState<DocState>(base);
  const [active, setActive] = useState<SourceField>(SOURCE_FIELD.skill);
  const [editing, setEditing] = useState(false);
  const [etag, setEtag] = useState(initialEtag);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [staleReloaded, setStaleReloaded] = useState(false);
  const [, startTransition] = useTransition();

  const activeBase = base[active];
  const activeDraft = draft[active];
  const changed = activeDraft !== activeBase;
  const diff = changed ? computeLineDiff(activeBase, activeDraft) : [];

  function enterEdit() {
    setError(null);
    setStaleReloaded(false);
    setEditing(true);
  }

  function cancelEdit() {
    setDraft(base);
    setEditing(false);
    setError(null);
    setStaleReloaded(false);
  }

  function setActiveDraft(value: string) {
    setDraft((prev) => ({ ...prev, [active]: value }));
  }

  // A 412 means another operator saved while this editor was open. Reload the
  // current source + ETag, keep the operator's draft, and re-diff against the
  // fresh base — never a silent overwrite (§4).
  async function reloadAfterStale() {
    const reloaded = await getFleetDetailAction(workspaceId, fleetId);
    if (!reloaded.ok) {
      setError(presentErrorString({ errorCode: reloaded.errorCode, message: reloaded.error, action: "reload the source" }));
      return;
    }
    const fresh = reloaded.data.fleet;
    setBase((prev) => ({
      ...prev,
      [SOURCE_FIELD.skill]: fresh.source_markdown,
      [SOURCE_FIELD.trigger]: fresh.trigger_markdown ?? "",
    }));
    setEtag(reloaded.data.etag);
    setStaleReloaded(true);
  }

  function onConfirmSave() {
    if (etag === null) return;
    const field = active;
    setError(null);
    startTransition(async () => {
      const result = await saveFleetSourceAction(
        workspaceId,
        fleetId,
        { [PATCH_FIELD[field]]: draft[field] },
        etag,
      );
      if (result.ok) {
        setBase((prev) => ({ ...prev, [field]: draft[field] }));
        setEtag(result.data.etag);
        setEditing(false);
        setDialogOpen(false);
        setStaleReloaded(false);
        captureProductEvent(EVENTS.fleet_source_saved, { fleet_id: fleetId, field, outcome: OUTCOME.success });
        router.refresh();
        return;
      }
      setDialogOpen(false);
      if (result.status === PRECONDITION_FAILED) {
        await reloadAfterStale();
        return;
      }
      captureProductEvent(EVENTS.fleet_source_saved, { fleet_id: fleetId, field, outcome: OUTCOME.failure });
      setError(presentErrorString({ errorCode: result.errorCode, message: result.error, action: "save the source" }));
    });
  }

  return (
    <Card className="flex flex-col gap-md bg-card p-4" aria-label={SOURCE_PANEL_TITLE}>
      <div className="flex items-center justify-between gap-md">
        <span className="font-mono text-sm font-medium text-foreground">{SOURCE_PANEL_TITLE}</span>
        {editing ? (
          <div className="flex items-center gap-xs">
            <Button type="button" variant="ghost" size="sm" onClick={cancelEdit}>
              {CANCEL_EDIT_LABEL}
            </Button>
            <Button type="button" variant="secondary" size="sm" disabled={!changed} onClick={() => setDialogOpen(true)}>
              {SAVE_SOURCE_LABEL}
            </Button>
          </div>
        ) : (
          <Button type="button" variant="outline" size="sm" onClick={enterEdit}>
            <PencilIcon size={14} /> {EDIT_SOURCE_LABEL}
          </Button>
        )}
      </div>

      <Tabs value={active} onValueChange={(v) => setActive(v as SourceField)}>
        <TabsList>
          <TabsTrigger value={SOURCE_FIELD.skill}>{SKILL_DOC_LABEL}</TabsTrigger>
          <TabsTrigger value={SOURCE_FIELD.trigger}>{TRIGGER_DOC_LABEL}</TabsTrigger>
        </TabsList>
        <TabsContent value={SOURCE_FIELD.skill} className="mt-md">
          <DocumentPane
            label={SKILL_DOC_LABEL}
            editing={editing}
            base={base[SOURCE_FIELD.skill]}
            draft={draft[SOURCE_FIELD.skill]}
            emptyHint=""
            onChange={setActiveDraft}
          />
        </TabsContent>
        <TabsContent value={SOURCE_FIELD.trigger} className="mt-md">
          <DocumentPane
            label={TRIGGER_DOC_LABEL}
            editing={editing}
            base={base[SOURCE_FIELD.trigger]}
            draft={draft[SOURCE_FIELD.trigger]}
            emptyHint={TRIGGER_DOC_EMPTY}
            onChange={setActiveDraft}
          />
        </TabsContent>
      </Tabs>

      {editing && changed ? <DiffPanel diff={diff} stale={staleReloaded} /> : null}
      {error ? <Alert variant="destructive">{error}</Alert> : null}

      <ConfirmDialog
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        title={SAVE_DIALOG_TITLE}
        description={SAVE_NEXT_WAKE_NOTICE}
        confirmLabel={SAVE_CONFIRM_LABEL}
        onConfirm={onConfirmSave}
      />
    </Card>
  );
}

// One document's viewer (read-only mono block + copy) or editor (Textarea). The
// active tab drives which document `onChange` writes to via the parent.
function DocumentPane({
  label,
  editing,
  base,
  draft,
  emptyHint,
  onChange,
}: {
  label: string;
  editing: boolean;
  base: string;
  draft: string;
  emptyHint: string;
  onChange: (value: string) => void;
}) {
  if (editing) {
    return (
      <Textarea
        aria-label={`Edit ${label}`}
        value={draft}
        onChange={(e) => onChange(e.target.value)}
        className="min-h-64 w-full resize-y font-mono text-xs leading-mono"
      />
    );
  }
  if (base.length === 0) {
    return <p className="text-sm text-muted-foreground">{emptyHint}</p>;
  }
  return (
    <div className="relative">
      <div className="absolute right-xs top-xs">
        <CopyButton value={base} label={`Copy ${label}`} />
      </div>
      <pre
        aria-label={label}
        className="max-h-96 overflow-auto rounded-md border border-border bg-muted/30 px-3 py-2 font-mono text-xs leading-mono text-foreground"
      >
        {base}
      </pre>
    </div>
  );
}

// The "what changes when you save" panel (§4): added/removed lines against the
// current source. Rendered only while editing with a pending change.
function DiffPanel({ diff, stale }: { diff: DiffLine[]; stale: boolean }) {
  return (
    <div className="flex flex-col gap-xs" data-testid="source-diff">
      <div className="flex items-center gap-md">
        <span className="font-mono text-eyebrow uppercase text-muted-foreground">{DIFF_PANEL_TITLE}</span>
        {stale ? <Badge variant="amber">reloaded</Badge> : null}
      </div>
      {stale ? <Alert variant="warning">{SAVE_STALE_RELOADED_NOTICE}</Alert> : null}
      {diff.length === 0 ? (
        <p className="text-xs text-muted-foreground">{DIFF_NO_CHANGES}</p>
      ) : (
        <div className="overflow-x-auto rounded-md border border-border bg-muted/30 font-mono text-xs leading-mono">
          {diff.map((line, i) => (
            <div
              key={`${line.kind}:${i}:${line.text}`}
              className={cn(
                "whitespace-pre px-3 py-0.5",
                line.kind === DIFF_KIND.add ? "text-success" : "text-destructive",
              )}
            >
              {line.kind === DIFF_KIND.add ? DIFF_ADDED_PREFIX : DIFF_REMOVED_PREFIX} {line.text}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ── the pending-change diff (§4) ──

const DIFF_KIND = { add: "add", remove: "remove" } as const;
type DiffKind = (typeof DIFF_KIND)[keyof typeof DIFF_KIND];
type DiffLine = { kind: DiffKind; text: string };

// A minimal line-level diff over the longest common subsequence: the lines
// present in the edited source but not the current one are additions; the
// reverse are removals. Equal inputs yield an empty diff — that is what the
// "no changes" branch keys on. No external diff dependency.
function computeLineDiff(base: string, next: string): DiffLine[] {
  const a = base.split("\n");
  const b = next.split("\n");
  const m = a.length;
  const n = b.length;
  // lcs[i][j] = LCS length of a[i:] and b[j:], as a flat row-major (m+1)×(n+1)
  // grid. `at` reads with a `?? 0` floor so no index access needs a non-null
  // assertion (oxlint forbids `!`); `line` does the same for the string reads.
  const width = n + 1;
  const lcs = new Array<number>((m + 1) * width).fill(0);
  const at = (i: number, j: number): number => lcs[i * width + j] ?? 0;
  const line = (arr: string[], i: number): string => arr[i] ?? "";
  for (let i = m - 1; i >= 0; i--) {
    for (let j = n - 1; j >= 0; j--) {
      lcs[i * width + j] = a[i] === b[j] ? at(i + 1, j + 1) + 1 : Math.max(at(i + 1, j), at(i, j + 1));
    }
  }
  const out: DiffLine[] = [];
  let i = 0;
  let j = 0;
  while (i < m && j < n) {
    if (a[i] === b[j]) {
      i++;
      j++;
    } else if (at(i + 1, j) >= at(i, j + 1)) {
      out.push({ kind: DIFF_KIND.remove, text: line(a, i) });
      i++;
    } else {
      out.push({ kind: DIFF_KIND.add, text: line(b, j) });
      j++;
    }
  }
  while (i < m) out.push({ kind: DIFF_KIND.remove, text: line(a, i++) });
  while (j < n) out.push({ kind: DIFF_KIND.add, text: line(b, j++) });
  return out;
}
