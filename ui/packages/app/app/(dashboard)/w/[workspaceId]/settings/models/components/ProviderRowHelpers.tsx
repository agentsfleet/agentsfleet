import { Button } from "@agentsfleet/design-system";
import { ArrowLeftRightIcon, PencilIcon, Trash2Icon } from "lucide-react";
import type { ProviderKeySecret } from "@/lib/api/secrets";
import ProviderEditPanel from "./ProviderEditPanel";

export const ANTHROPIC_PROVIDER = "anthropic";
export const ADD_KEY_AND_MODEL_LABEL = "Add key";
export const DEFAULT_LABEL = "Default";
export const CUSTOM_LABEL = "OpenAI-compatible";
export const PLATFORM_UNAVAILABLE_NOTE = "No default is configured.";

export const PANEL = { addKey: "addKey", edit: "edit" } as const;
export type PanelKind = (typeof PANEL)[keyof typeof PANEL];
export type OpenRow = "anthropic" | "other" | "custom" | null;

// Threshold + divisor for the "k" context abbreviation (200000 → "200k").
const TOKENS_PER_K = 1000;

export function formatContext(tokens: number | undefined): string {
  if (!tokens || tokens <= 0) return "default";
  return tokens >= TOKENS_PER_K ? `${Math.round(tokens / TOKENS_PER_K)}k` : String(tokens);
}

export function LiveBadge() {
  return (
    <span className="inline-flex items-center gap-1 text-xs font-semibold text-pulse">
      <span className="h-1.5 w-1.5 rounded-full bg-pulse" aria-hidden="true" />
      Live
    </span>
  );
}

// Shared plumbing every non-Default row needs to open its own inline panel
// and drive the switch/delete server actions — bundled so each row's props
// stay a single object instead of a handful of positional callbacks.
export type RowControls = {
  workspaceId: string;
  pending: boolean;
  openRow: OpenRow;
  openPanel: PanelKind;
  toggle: (row: OpenRow, panel: PanelKind) => void;
  close: () => void;
  onSwitch: (secretRef: string, model?: string) => void;
  onDelete: (name: string) => void;
};

export function switchButton(pending: boolean, onClick: () => void, disabled?: boolean, title?: string) {
  return (
    <Button
      type="button"
      size="sm"
      disabled={pending || disabled}
      onClick={onClick}
      title={title}
      className="gap-1.5"
    >
      <ArrowLeftRightIcon size={14} />
      Switch
    </Button>
  );
}

export function deleteButton(
  pending: boolean,
  onDelete: (name: string) => void,
  name: string,
  disabled: boolean,
) {
  return (
    <Button
      type="button"
      variant="destructive"
      size="sm"
      disabled={pending || disabled}
      onClick={() => onDelete(name)}
      aria-label={disabled ? `Cannot delete ${name} while it is active` : `Delete ${name}`}
    >
      <Trash2Icon size={14} />
    </Button>
  );
}

export function addButton(pending: boolean, expanded: boolean, label: string, onClick: () => void) {
  return (
    <Button
      type="button"
      size="sm"
      variant="outline"
      disabled={pending}
      aria-expanded={expanded}
      onClick={onClick}
      className="gap-1.5"
    >
      <PencilIcon size={14} />
      {label}
    </Button>
  );
}

export function editButtons(row: "anthropic" | "other", secret: ProviderKeySecret, controls: RowControls) {
  return (
    <div className="flex items-center gap-1">
      <Button
        type="button"
        variant="outline"
        size="sm"
        aria-label="Edit"
        aria-expanded={controls.openRow === row && controls.openPanel === PANEL.edit}
        onClick={() => controls.toggle(row, PANEL.edit)}
      >
        <PencilIcon size={14} />
      </Button>
      {deleteButton(controls.pending, controls.onDelete, secret.name, true)}
    </div>
  );
}

export function editPanel(
  row: "anthropic" | "other",
  secret: ProviderKeySecret,
  model: string,
  controls: RowControls,
) {
  if (controls.openRow !== row || controls.openPanel !== PANEL.edit) return null;
  return (
    <ProviderEditPanel
      workspaceId={controls.workspaceId}
      provider={secret.provider}
      secretRef={secret.name}
      currentModel={model}
      onClose={controls.close}
    />
  );
}
