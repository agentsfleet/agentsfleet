"use client";

import type { FleetSettings } from "@/components/domain/fleet/FleetSettings";
import { useEffect, useId, useRef, useState } from "react";
import { StackIcon } from "@phosphor-icons/react/ssr";
import { Button, Input, PageTitle, cn } from "@agentsfleet/design-system";
import { FLEET_NAME_MAX_BYTES, fleetNameFromLabel } from "@/lib/chat-address";

type Edit = {text: string; error: string | null};
const NAME_FORMAT = `Use up to ${FLEET_NAME_MAX_BYTES} letters, numbers, spaces or hyphens.`;
const SAVED_TINT_MS = 700;

export function FleetTitle({name, rename}: {name: string; rename: ReturnType<typeof useFleetRename>}) {
  const errorId = useId();
  const {edit, canRename, begin, cancel, submit, update, input, title, saving, saved} = rename;
  return <div className="flex min-w-0 items-center gap-md summary-medium:gap-lg">
    <StackIcon data-app-header-icon="" size={16} className="hidden shrink-0 text-muted-foreground sm:block" aria-hidden="true" />
    {edit ? <div className="min-w-0">
      <Input ref={input} className="field-sizing-content min-h-0 w-auto min-w-0 max-w-full truncate border-0 bg-transparent p-0 text-chat-title leading-label font-semibold tracking-label caret-primary focus-visible:underline focus-visible:ring-0 focus-visible:ring-offset-0 pointer-coarse:min-h-0"
        aria-label="Fleet name" aria-describedby={edit.error ? errorId : undefined} aria-invalid={edit.error !== null}
        autoComplete="off" spellCheck={false} readOnly={saving} value={edit.text} onChange={event => update(event.target.value)}
        onBlur={() => {if (!edit.error) void submit(false);}} onKeyDown={event => {
          if (event.nativeEvent.isComposing || saving) return;
          if (event.key === "Escape") {event.preventDefault(); cancel(true);}
          if (event.key === "Enter") {event.preventDefault(); void submit(true);}
        }} />
      {edit.error ? <p id={errorId} role="alert" className="wrap-anywhere text-eyebrow text-destructive">{edit.error}</p> : null}
    </div> : <PageTitle data-app-header-title="" size="chat" className="min-w-0 truncate font-semibold">
      {canRename ? <Button ref={title} variant="ghost" size="compact" title="Rename fleet" onClick={begin}
        className={cn("block min-h-0 max-w-full cursor-text truncate p-0 text-chat-title leading-label font-semibold tracking-label text-foreground hover:bg-transparent hover:underline focus-visible:ring-inset", saved && "text-success")}
        onKeyDown={event => {if (event.key === "F2") {event.preventDefault(); begin();}}}>{name}</Button> : name}
    </PageTitle>}
    <output className="sr-only">{saving ? "Saving fleet name…" : saved ? "Fleet renamed." : ""}</output>
  </div>;
}

export function useFleetRename(name: string, settings: FleetSettings | null) {
  const [edit, setEdit] = useState<Edit | null>(null);
  const [saved, setSaved] = useState(false);
  const title = useRef<HTMLButtonElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const restoreTitle = useRef(false);
  const submitting = useRef(false);
  const editing = edit !== null;
  useEffect(() => {
    if (editing) {
      const field = input.current;
      field?.focus({preventScroll: true});
      field?.setSelectionRange(field.value.length, field.value.length);
    } else if (restoreTitle.current) {
      restoreTitle.current = false;
      title.current?.focus({preventScroll: true});
    }
  }, [editing]);
  useEffect(() => {
    if (!saved) return;
    const timer = setTimeout(() => setSaved(false), SAVED_TINT_MS);
    return () => clearTimeout(timer);
  }, [saved]);

  const begin = () => {
    setSaved(false);
    if (editing) input.current?.focus();
    else setEdit({text: name, error: null});
  };
  const cancel = (restore: boolean) => {restoreTitle.current = restore; setEdit(null);};
  const saving = settings?.pending === "name";
  const submit = async (restore: boolean): Promise<void> => {
    if (!settings || !edit || submitting.current) return;
    const next = fleetNameFromLabel(edit.text);
    if (!edit.text.trim() || next === name) {cancel(restore); return;}
    if (!next) {setEdit({...edit, error: NAME_FORMAT}); return;}
    submitting.current = true;
    setEdit({...edit, error: null});
    try {
      const outcome = await settings.change("name", {name: next});
      if (outcome?.ok) {
        cancel(restore && document.activeElement === input.current);
        setSaved(true);
      } else setEdit({...edit, error: outcome?.message ?? "Wait for the other fleet change to finish and try again."});
    } finally {
      submitting.current = false;
    }
  };
  return {edit: settings ? edit : null, canRename: settings !== null, begin, cancel, submit, input, title, saving, saved,
    update: (text: string) => setEdit({text, error: null})};
}
