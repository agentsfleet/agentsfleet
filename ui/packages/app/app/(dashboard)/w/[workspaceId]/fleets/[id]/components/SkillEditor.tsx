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
} from "@agentsfleet/design-system";
import { getFleetDetailAction, saveFleetSourceAction } from "../../actions";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { EVENTS } from "@/lib/analytics/events";
import { presentErrorString } from "@/lib/errors";
import {
  CANCEL_EDIT_LABEL,
  DIFF_CURRENT_LABEL,
  DIFF_PANEL_TITLE,
  DIFF_PENDING_LABEL,
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

// HTTP 412 Precondition Failed means the source changed under an open editor.
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

function sourceState(sourceMarkdown: string, triggerMarkdown: string | null): DocState {
  return {
    [SOURCE_FIELD.skill]: sourceMarkdown,
    [SOURCE_FIELD.trigger]: triggerMarkdown ?? "",
  };
}

export default function SkillEditor({
  workspaceId,
  fleetId,
  sourceMarkdown,
  triggerMarkdown,
  etag: initialEtag,
}: Props) {
  const router = useRouter();
  const [base, setBase] = useState<DocState>(() => sourceState(sourceMarkdown, triggerMarkdown));
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

  // Keep the operator's draft while refreshing the current source after a
  // concurrent save. This prevents a silent overwrite.
  async function reloadAfterStale() {
    const reloaded = await getFleetDetailAction(workspaceId, fleetId);
    if (!reloaded.ok) {
      setError(presentErrorString({ errorCode: reloaded.errorCode, message: reloaded.error, action: "reload the source" }));
      return;
    }
    const fresh = reloaded.data.fleet;
    setBase(sourceState(fresh.source_markdown, fresh.trigger_markdown));
    setEtag(reloaded.data.etag);
    setStaleReloaded(true);
  }

  function onConfirmSave() {
    if (etag === null) return;
    const field = active;
    setError(null);
    startTransition(async () => {
      const result = await saveFleetSourceAction(workspaceId, fleetId, { [PATCH_FIELD[field]]: draft[field] }, etag);
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

      {editing && changed ? <ChangePreview current={activeBase} pending={activeDraft} stale={staleReloaded} /> : null}
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

function ChangePreview({ current, pending, stale }: { current: string; pending: string; stale: boolean }) {
  return (
    <div className="flex flex-col gap-xs" data-testid="source-diff">
      <div className="flex items-center gap-md">
        <span className="font-mono text-eyebrow uppercase text-muted-foreground">{DIFF_PANEL_TITLE}</span>
        {stale ? <Badge variant="amber">reloaded</Badge> : null}
      </div>
      {stale ? <Alert variant="warning">{SAVE_STALE_RELOADED_NOTICE}</Alert> : null}
      <div className="grid gap-sm">
        <SourcePreview label={DIFF_CURRENT_LABEL} value={current} />
        <SourcePreview label={DIFF_PENDING_LABEL} value={pending} />
      </div>
    </div>
  );
}

function SourcePreview({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex min-w-0 flex-col gap-xs">
      <span className="text-xs text-muted-foreground">{label}</span>
      <pre className="max-h-48 overflow-auto rounded-md border border-border bg-muted/30 px-3 py-2 font-mono text-xs leading-mono">
        {value}
      </pre>
    </div>
  );
}
