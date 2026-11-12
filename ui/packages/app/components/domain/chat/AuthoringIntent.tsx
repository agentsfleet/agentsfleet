"use client";

import { createContext, useContext, useEffect, useRef } from "react";
import { useAui } from "@assistant-ui/react";

/** An explicit navigation choice, separate from a confirmed publication receipt. */
export type AuthoringIntent = {prompt?: string; publicationTarget?: string};
export const AuthoringIntentContext = createContext<AuthoringIntent>({});
export const useAuthoringIntent = () => useContext(AuthoringIntentContext);

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
