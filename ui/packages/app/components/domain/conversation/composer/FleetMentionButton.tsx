"use client";

import type {RefObject} from "react";
import {flushSync} from "react-dom";
import {useAui} from "@assistant-ui/react";
import {AtIcon} from "@phosphor-icons/react/ssr";
import {Button} from "@agentsfleet/design-system";
import {activeFleetMention} from "@/lib/chat-address";

export function FleetMentionButton({input, disabled, inSetup = false}: {
  input: RefObject<HTMLTextAreaElement | null>; disabled: boolean; inSetup?: boolean;
}) {
  const aui = useAui();
  const label = inSetup ? "Mention a fleet in setup" : "Address a workspace fleet";
  function mention() {
    const field = input.current;
    if (!field) return;
    const {selectionStart, selectionEnd, value} = field;
    const recipient = !inSetup && /^\s*@[a-z\d-]+(?=[\s,:]|$)/i.exec(value);
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
    className="fleet-composer-tool"
    aria-label={label} title={label} onClick={mention}><AtIcon aria-hidden="true" size={18} /></Button>;
}
