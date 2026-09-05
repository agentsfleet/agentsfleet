"use client";

import { useId, useLayoutEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { DocumentPane, StaleSourceComparison } from "./SourceDocumentPane";
import { PencilIcon } from "lucide-react";
import {
  Alert,
  Button,
  Card,
  ConfirmDialog,
  cn,
} from "@agentsfleet/design-system";
import { getFleetDetailAction, saveFleetSourceAction } from "../../actions";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { EVENTS } from "@/lib/analytics/events";
import { presentErrorString } from "@/lib/errors";
import {
  CANCEL_EDIT_LABEL,
  EDIT_SOURCE_LABEL,
  HIDE_SOURCE_LABEL,
  OUTCOME,
  SAVE_CONFIRM_LABEL,
  SAVE_DIALOG_TITLE,
  SAVE_NEXT_WAKE_NOTICE,
  SAVE_SOURCE_LABEL,
  SAVE_STALE_RELOADED_NOTICE,
  SKILL_DOC_LABEL,
  SKILL_SOURCE_PANEL_TITLE,
  SOURCE_FIELD,
  TRIGGER_DOC_EMPTY,
  TRIGGER_DOC_LABEL,
  TRIGGER_SOURCE_PANEL_TITLE,
  VIEW_SOURCE_LABEL,
  type SourceField,
} from "./console-copy";

const PRECONDITION_FAILED = 412;

type Props = {
  workspaceId: string;
  fleetId: string;
  field: SourceField;
  sourceMarkdown: string;
  triggerMarkdown: string | null;
  etag: string;
  fillAvailableSpace?: boolean;
};

const PATCH_FIELD: Record<SourceField, "source_markdown" | "trigger_markdown"> = {
  [SOURCE_FIELD.skill]: "source_markdown",
  [SOURCE_FIELD.trigger]: "trigger_markdown",
};

function documentValue(
  field: SourceField,
  sourceMarkdown: string,
  triggerMarkdown: string | null,
): string {
  return field === SOURCE_FIELD.skill ? sourceMarkdown : (triggerMarkdown ?? "");
}

export default function SkillEditor({
  workspaceId,
  fleetId,
  field,
  sourceMarkdown,
  triggerMarkdown,
  etag: initialEtag,
  fillAvailableSpace = false,
}: Props) {
  const router = useRouter();
  const panelId = useId();
  const initial = documentValue(field, sourceMarkdown, triggerMarkdown);
  const [base, setBase] = useState(initial);
  const [draft, setDraft] = useState(initial);
  const [editing, setEditing] = useState(false);
  const [expanded, setExpanded] = useState(true);
  const [etag, setEtag] = useState(initialEtag);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [staleReloaded, setStaleReloaded] = useState(false);
  const editingRef = useRef(editing);
  const fieldRef = useRef(field);
  editingRef.current = editing;

  useLayoutEffect(() => {
    const fresh = documentValue(field, sourceMarkdown, triggerMarkdown);
    const sameField = fieldRef.current === field;
    setBase(fresh);
    setEtag(initialEtag);
    setDraft((previous) => editingRef.current && sameField ? previous : fresh);
    if (!sameField) {
      setEditing(false);
      setExpanded(false);
      setError(null);
      setStaleReloaded(false);
    }
    fieldRef.current = field;
  }, [field, sourceMarkdown, triggerMarkdown, initialEtag]);

  const changed = draft !== base;
  const label = field === SOURCE_FIELD.skill ? SKILL_DOC_LABEL : TRIGGER_DOC_LABEL;
  const title = field === SOURCE_FIELD.skill
    ? SKILL_SOURCE_PANEL_TITLE
    : TRIGGER_SOURCE_PANEL_TITLE;

  function enterEdit() {
    setError(null);
    setStaleReloaded(false);
    setExpanded(true);
    setEditing(true);
  }

  function cancelEdit() {
    setDraft(base);
    setEditing(false);
    setError(null);
    setStaleReloaded(false);
  }

  async function reloadAfterStale() {
    const reloaded = await getFleetDetailAction(workspaceId, fleetId);
    if (!reloaded.ok) {
      setError(presentErrorString({
        errorCode: reloaded.errorCode,
        message: reloaded.error,
        action: "reload the source",
      }));
      return;
    }
    const fresh = documentValue(
      field,
      reloaded.data.fleet.source_markdown,
      reloaded.data.fleet.trigger_markdown,
    );
    setBase(fresh);
    setEtag(reloaded.data.etag);
    setStaleReloaded(true);
  }

  async function onConfirmSave() {
    setError(null);
    const result = await saveFleetSourceAction(
      workspaceId,
      fleetId,
      { [PATCH_FIELD[field]]: draft },
      etag,
    );
    if (result.ok) {
      setBase(draft);
      setEtag(result.data.etag);
      setEditing(false);
      setDialogOpen(false);
      setStaleReloaded(false);
      captureProductEvent(EVENTS.fleet_source_saved, {
        fleet_id: fleetId,
        field,
        outcome: OUTCOME.success,
      });
      router.refresh();
      return;
    }
    setDialogOpen(false);
    if (result.status === PRECONDITION_FAILED) {
      await reloadAfterStale();
      return;
    }
    captureProductEvent(EVENTS.fleet_source_saved, {
      fleet_id: fleetId,
      field,
      outcome: OUTCOME.failure,
    });
    setError(presentErrorString({
      errorCode: result.errorCode,
      message: result.error,
      action: "save the source",
    }));
  }

  return (
    <Card
      className={cn("flex flex-col gap-md bg-card p-4", fillAvailableSpace && "min-h-0 flex-1")}
      aria-label={title}
    >
      <div className="flex items-center justify-between gap-md">
        <span className="font-sans text-sm font-medium text-foreground">{title}</span>
        <EditorActions
          editing={editing}
          expanded={expanded}
          changed={changed}
          panelId={panelId}
          onCancel={cancelEdit}
          onSave={() => setDialogOpen(true)}
          onToggle={() => setExpanded((value) => !value)}
          onEdit={enterEdit}
        />
      </div>

      {expanded || editing ? (
        <div id={panelId} className={cn("flex flex-col gap-md", fillAvailableSpace && "min-h-0 flex-1")}>
          <DocumentPane
            label={label}
            editing={editing}
            value={editing ? draft : base}
            emptyHint={field === SOURCE_FIELD.trigger ? TRIGGER_DOC_EMPTY : ""}
            onChange={setDraft}
            fillAvailableSpace={fillAvailableSpace}
          />
          {staleReloaded ? <Alert variant="warning">{SAVE_STALE_RELOADED_NOTICE}</Alert> : null}
          {staleReloaded ? <StaleSourceComparison base={base} draft={draft} /> : null}
          {error ? <Alert variant="destructive">{error}</Alert> : null}
        </div>
      ) : null}

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

function EditorActions({
  editing,
  expanded,
  changed,
  panelId,
  onCancel,
  onSave,
  onToggle,
  onEdit,
}: {
  editing: boolean;
  expanded: boolean;
  changed: boolean;
  panelId: string;
  onCancel: () => void;
  onSave: () => void;
  onToggle: () => void;
  onEdit: () => void;
}) {
  if (editing) {
    return (
      <div className="flex items-center gap-xs">
        <Button type="button" variant="ghost" size="sm" onClick={onCancel}>
          {CANCEL_EDIT_LABEL}
        </Button>
        <Button type="button" variant="secondary" size="sm" disabled={!changed} onClick={onSave}>
          {SAVE_SOURCE_LABEL}
        </Button>
      </div>
    );
  }
  return (
    <div className="flex items-center gap-xs">
      <Button
        type="button"
        variant="ghost"
        size="sm"
        aria-expanded={expanded}
        aria-controls={panelId}
        onClick={onToggle}
      >
        {expanded ? HIDE_SOURCE_LABEL : VIEW_SOURCE_LABEL}
      </Button>
      <Button type="button" variant="outline" size="sm" onClick={onEdit}>
        <PencilIcon size={14} /> {EDIT_SOURCE_LABEL}
      </Button>
    </div>
  );
}
