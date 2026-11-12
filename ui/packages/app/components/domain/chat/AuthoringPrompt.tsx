"use client";

import { useEffect, useRef } from "react";
import { useAui } from "@assistant-ui/react";
import { useAuthoringIntent } from "./AuthoringIntent";

export function AuthoringPrompt({hasMessages}: {hasMessages: boolean}) {
  const {prompt} = useAuthoringIntent();
  const aui = useAui();
  const initialized = useRef(false);
  useEffect(() => {
    if (initialized.current) return;
    initialized.current = true;
    if (prompt && !hasMessages && !aui.composer().getState().text) aui.composer().setText(prompt);
  }, [aui, prompt, hasMessages]);
  return null;
}
