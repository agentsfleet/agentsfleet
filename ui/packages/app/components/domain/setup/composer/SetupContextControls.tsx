"use client";

import {useRef, useState, type RefObject} from "react";
import {useAui} from "@assistant-ui/react";
import {PaperclipIcon} from "@phosphor-icons/react/ssr";
import {Button} from "@agentsfleet/design-system";
import {FleetMentionButton} from "../../conversation/composer/FleetMentionButton";

export function SetupContextControls({input, disabled}: {input: RefObject<HTMLTextAreaElement | null>; disabled: boolean}) {
  const aui = useAui();
  const filePicker = useRef<HTMLInputElement>(null);
  const [notice, setNotice] = useState("");
  async function insertFile(file: File) {
    const frame = input.current;
    const original = aui.composer().getState().text;
    try {
      const notes = await file.text();
      if (!frame?.isConnected || frame.disabled) return;
      if (!notes.trim() || notes.includes("\u0000") || notes.includes("\ufffd")) return setNotice("Choose a non-empty plain text document.");
      const current = aui.composer().getState().text;
      if (original !== current) return setNotice("Your draft changed. Choose the document again to insert its text.");
      const combined = [current, file.name + "\n" + notes].filter(Boolean).join("\n\n");
      aui.composer().setText(combined);
      setNotice("");
      frame.focus();
    } catch { setNotice("Couldn’t read this document. Choose it again."); }
  }
  return <>
    <input ref={filePicker} type="file" className="hidden" accept="text/*,.md,.txt,.csv,.json,.yaml,.yml,.log"
      aria-label="Choose a text document" onChange={event => {
        const file = event.currentTarget.files?.[0];
        event.currentTarget.value = "";
        if (file) void insertFile(file);
      }} />
    <Button type="button" variant="ghost" size="icon-row" disabled={disabled} className="text-muted-foreground pointer-coarse:size-11 [&>svg]:size-4.5"
      aria-label="Insert a text document into setup" title="Insert an editable text document" onClick={() => filePicker.current?.click()}>
      <PaperclipIcon aria-hidden="true" size={18} />
    </Button>
    <FleetMentionButton input={input} disabled={disabled} inSetup />
    {notice && <output className="text-eyebrow text-warning">{notice}</output>}
  </>;
}
