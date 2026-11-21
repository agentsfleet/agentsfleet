"use client";

import {useRef, useState, type RefObject} from "react";
import {flushSync} from "react-dom";
import {useAui} from "@assistant-ui/react";
import {AtIcon, PaperclipIcon} from "@phosphor-icons/react/ssr";
import {Button} from "@agentsfleet/design-system";
import {MESSAGE_MAX_BYTES, MessageInputSchema} from "@/lib/api/draft-types";

const CONTROL = "size-9 shrink-0 text-muted-foreground pointer-coarse:size-11";
const TOO_LARGE = "The setup message can hold 32 KiB. Choose a shorter note or shorten your draft.";

export function SetupContextControls({input, disabled}: {input: RefObject<HTMLTextAreaElement | null>; disabled: boolean}) {
  const aui = useAui();
  const filePicker = useRef<HTMLInputElement>(null);
  const [notice, setNotice] = useState("");
  async function insertFile(file: File) {
    if (file.size > MESSAGE_MAX_BYTES) return setNotice(TOO_LARGE);
    const frame = input.current;
    const original = aui.composer().getState().text;
    try {
      const notes = await file.text();
      if (!frame?.isConnected || frame.disabled) return;
      if (!notes.trim() || notes.includes("\u0000") || notes.includes("\ufffd")) return setNotice("Choose a non-empty plain text document.");
      const current = aui.composer().getState().text;
      if (original !== current) return setNotice("Your draft changed. Choose the document again to insert its text.");
      const combined = [current, file.name + "\n" + notes].filter(Boolean).join("\n\n");
      if (!MessageInputSchema.safeParse(combined).success) return setNotice(TOO_LARGE);
      aui.composer().setText(combined);
      setNotice("");
      frame.focus();
    } catch { setNotice("Couldn’t read this document. Choose it again."); }
  }
  function mention() {
    const field = input.current;
    if (!field) return;
    const {selectionStart, selectionEnd, value} = field;
    const prefix = value.slice(0, selectionStart);
    if (selectionStart !== selectionEnd || !/(?:^|\s)@[a-z\d-]{0,64}$/i.test(prefix)) {
      const mentionText = prefix && !/\s$/.test(prefix) ? " @" : "@";
      field.setRangeText(mentionText, selectionStart, selectionEnd, "end");
      const caret = selectionStart + mentionText.length;
      flushSync(() => aui.composer().setText(field.value));
      field.setSelectionRange(caret, caret);
    }
    field.focus();
  }
  return <>
    <input ref={filePicker} type="file" className="hidden" accept="text/*,.md,.txt,.csv,.json,.yaml,.yml,.log"
      aria-label="Choose a text document" onChange={event => {
        const file = event.currentTarget.files?.[0];
        event.currentTarget.value = "";
        if (file) void insertFile(file);
      }} />
    <Button type="button" variant="ghost" size="icon" disabled={disabled} className={CONTROL}
      aria-label="Insert a text document into setup" title="Insert an editable text document" onClick={() => filePicker.current?.click()}>
      <PaperclipIcon aria-hidden="true" size={16} />
    </Button>
    <Button type="button" variant="ghost" size="icon" disabled={disabled} className={CONTROL}
      aria-label="Mention a fleet in setup" title="Mention a fleet in setup" onClick={mention}><AtIcon aria-hidden="true" size={16} /></Button>
    {notice && <output className="text-eyebrow text-warning">{notice}</output>}
  </>;
}
