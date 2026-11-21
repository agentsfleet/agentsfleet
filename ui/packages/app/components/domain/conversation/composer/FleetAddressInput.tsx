"use client";

import type { ChangeEvent, KeyboardEvent, RefObject } from "react";
import type { NativeFleetSummary } from "@/lib/api/fleet-detail";
import { useEffect, useId, useRef, useState, useSyncExternalStore } from "react";
import { usePathname } from "next/navigation";
import { useAui, useAuiState } from "@assistant-ui/react";
import { Button, DashboardPanel, SectionLabel, SuggestionList } from "@agentsfleet/design-system";
import { ChatComposerInput } from "@/components/domain/shared/chat/ChatComposerFrame";
import { FleetListSchema } from "@/lib/api/fleet-detail";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import { fleetsPath } from "@/lib/api/fleet-paths";
import { useRecentPage } from "@/lib/api/use-recent-page";
import { workspaceIdFromPath } from "@/lib/workspace-routes";

type Props = {inputRef: RefObject<HTMLTextAreaElement | null>; placeholder: string; disabled: boolean};
type Picker = ReturnType<typeof useFleetAddress>;
type KeyControls = {
  open: boolean; inputRef: Props["inputRef"]; choices: readonly NativeFleetSummary[]; selected: number;
  choose: (name: string) => void; dismiss: () => void; move: (index: number) => void;
};

const EMPTY = {items: [], loading: true, more: false, error: null};
const emptySnapshot = () => EMPTY;
const emptySubscribe = () => () => {};

export function FleetAddressInput(props: Props) {
  const picker = useFleetAddress(props);
  const listId = useId();
  const listRef = useRef<HTMLDivElement>(null);
  const activeId = picker.open && picker.selected >= 0 ? `${listId}-${picker.selected}` : undefined;
  useEffect(() => {
    const row = activeId && listRef.current?.ownerDocument.getElementById(activeId);
    if (row) row.scrollIntoView({block: "nearest"});
  }, [activeId]);

  return <div className="relative flex min-w-0 flex-1" onKeyDownCapture={picker.keyDown}
    onFocus={() => picker.focus(true)} onBlur={event => {if (!event.currentTarget.contains(event.relatedTarget)) picker.focus(false);}}>
    <ChatComposerInput {...props}
      aria-autocomplete="list" aria-controls={picker.open ? listId : undefined} aria-activedescendant={activeId}
      onSelect={event => picker.caret(event.currentTarget.selectionEnd)} onChange={picker.change} />
    {picker.open ? <DashboardPanel padding="none" className="absolute inset-x-0 bottom-full z-20 mb-sm max-h-[min(16rem,40dvh)] overflow-y-auto rounded-lg border border-border bg-popover p-sm text-foreground shadow-floating" ref={listRef}>
      <SectionLabel variant="navigation" className="block px-md py-sm">Workspace fleets</SectionLabel>
      <SuggestionList id={listId} label="Workspace fleets" active={picker.selected} onChoose={picker.choose}
        options={picker.choices.map(fleet => ({key: fleet.id, value: fleet.name, label: `@${fleet.name}`}))} />
      <PickerStatus picker={picker} />
    </DashboardPanel> : null}
  </div>;
}

function useFleetAddress({inputRef, disabled}: Props) {
  const workspace = workspaceIdFromPath(usePathname());
  const aui = useAui();
  const text = useAuiState(state => state.composer.text);
  const [caret, setCaret] = useState(0);
  const [focused, setFocused] = useState(false);
  const [dismissed, setDismissed] = useState<string | null>(null);
  const [index, setIndex] = useState(0);
  // The wire format has one recipient. Never replace a recipient already chosen.
  const prefix = text.slice(0, caret);
  const mention = /(?:^|\s)@([a-z\d-]{0,64})$/i.exec(prefix);
  const open = Boolean(workspace && focused && !disabled && mention && dismissed !== text
    && !/^\s*@[a-z\d-]+[\s,:]/i.test(prefix));
  const query = mention?.[1] ?? "";
  const collection = useRecentPage(open && workspace ? `/live${fleetsPath(workspace)}` : null, FleetListSchema);
  const page = useSyncExternalStore(collection?.subscribe ?? emptySubscribe,
    collection?.getSnapshot ?? emptySnapshot, emptySnapshot);
  const choices = page.items.filter(fleet => fleet.status === AGENTSFLEET_STATUS.ACTIVE && fleet.name.toLowerCase().startsWith(query.toLowerCase()));
  const selected = Math.min(index, choices.length - 1);
  function choose(name: string) {
    // Keep the author's words when inserting an address in the middle of a draft.
    const before = text.slice(0, caret - query.length - 1).trimStart();
    const after = text.slice(caret);
    const words = [before.trimEnd(), after.trimStart()].filter(Boolean).join(" ");
    const addressed = `@${name} ${words}`;
    // Native selection events may still describe the old cursor after the text changes.
    setCaret(addressed.length);
    setDismissed(addressed);
    aui.composer().setText(addressed);
    inputRef.current?.focus();
    setIndex(0);
  }
  function keyDown(event: KeyboardEvent<HTMLDivElement>) {
    handleAddressKey(event, {open, inputRef, choices, selected, choose,
      dismiss: () => setDismissed(text), move: setIndex});
  }
  function change(event: ChangeEvent<HTMLTextAreaElement>) {
    setCaret(event.currentTarget.selectionEnd); setIndex(0); setDismissed(null);
  }
  return {open, page, choices, selected, collection, choose, keyDown, change, focus: setFocused, caret: setCaret};
}

function handleAddressKey(event: KeyboardEvent<HTMLDivElement>, controls: KeyControls) {
  const {open, inputRef, choices, selected, choose, dismiss, move} = controls;
  if (!open || event.nativeEvent.isComposing || event.target !== inputRef.current) return;
  const choice = choices[selected];
  if (event.key === "Escape") {
    event.preventDefault(); event.stopPropagation(); dismiss(); return;
  }
  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    event.preventDefault(); event.stopPropagation();
    move(choices.length ? (selected + (event.key === "ArrowDown" ? 1 : -1) + choices.length) % choices.length : 0);
  } else if (event.key === "Enter" && !event.shiftKey && choice) {
    event.preventDefault(); event.stopPropagation();
    choose(choice.name);
  }
}

function PickerStatus({picker}: {picker: Picker}) {
  const {page, choices, collection} = picker;
  return <>
      {page.loading || page.error || !choices.length ? <output className="px-md py-sm text-label text-muted-foreground">
        {page.error ?? (page.loading ? "Loading fleets…" : page.more ? "No matches on this page." : "No matching fleets.")}
      </output> : null}
      {page.error || page.more ? <Button type="button" size="sm" variant="ghost" disabled={page.loading}
        onClick={() => void collection?.load(!page.error)}>{page.error ? "Retry" : "Load more fleets"}</Button> : null}
  </>;
}
