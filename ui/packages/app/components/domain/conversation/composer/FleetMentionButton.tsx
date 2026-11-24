"use client";

import type {RefObject} from "react";
import {flushSync} from "react-dom";
import {useAui} from "@assistant-ui/react";
import {AtIcon} from "@phosphor-icons/react/ssr";
import {Button} from "@agentsfleet/design-system";
import {activeFleetMention} from "@/lib/chat-address";

/** `inline`: the mention goes where the caret is, never in front as the message's recipient. */
export function FleetMentionButton({input, disabled, inline}: {
  input: RefObject<HTMLTextAreaElement | null>; disabled: boolean; inline: boolean;
}) {
  const aui = useAui();
  const label = inline ? "Mention a fleet" : "Address a workspace fleet";
  function mention() {
    const field = input.current;
    if (!field) return;
    const {selectionStart, selectionEnd, value} = field;
    const recipient = !inline && /^\s*@[a-z\d-]+(?=[\s,:]|$)/i.exec(value);
    if (recipient) {
      field.setSelectionRange(recipient[0].length, recipient[0].length);
      field.focus();
      return;
    }
    const prefix = value.slice(0, selectionStart);
    if (selectionStart !== selectionEnd || activeFleetMention(prefix) === undefined) {
      const mentionText = prefix && !/\s$/.test(prefix) ? " @" : "@";
      // Keep the next word outside the new mention when inserting before existing text.
      const separator = /^[a-z\d-]/i.test(value.slice(selectionEnd)) ? " " : "";
      field.setRangeText(mentionText + separator, selectionStart, selectionEnd, "end");
      const caret = selectionStart + mentionText.length;
      flushSync(() => aui.composer().setText(field.value));
      field.setSelectionRange(caret, caret);
    }
    field.focus();
  }
  return <Button type="button" variant="ghost" size="icon-row" disabled={disabled}
    className="text-muted-foreground pointer-coarse:size-11 [&>svg]:size-4.5"
    aria-label={label} title={label} onClick={mention}><AtIcon aria-hidden="true" size={18} /></Button>;
}
