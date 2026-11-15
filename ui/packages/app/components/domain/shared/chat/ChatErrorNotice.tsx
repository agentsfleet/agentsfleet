"use client";

import type {ReactNode} from "react";
import {useEffect, useState} from "react";
import {createPortal} from "react-dom";
import {Alert, Toast, TooltipButton, cn, useResettableTimeout} from "@agentsfleet/design-system";
import {WarningCircleIcon, XIcon} from "@phosphor-icons/react/ssr";

const ERROR_NOTICE_MS = 6_000;

/** Closing the flash never clears the failed operation or its recovery actions. */
export function ChatErrorNotice({title, children, actions}: {title: string; children: ReactNode; actions: ReactNode}) {
  const [canvas, setCanvas] = useState<Element | null>(null);
  const [visible, setVisible] = useState(true);
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  const timer = useResettableTimeout();
  useEffect(() => {setCanvas(document.querySelector(".app-dashboard-canvas") ?? document.body);}, []);
  useEffect(() => {
    if (visible && !hovered && !focused) timer.start(() => setVisible(false), ERROR_NOTICE_MS);
    else timer.cancel();
    return timer.cancel;
  }, [visible, hovered, focused, timer]);
  const flash = <Toast visible={visible} severity="warning" aria-live="off" aria-label="Chat error notice" inert={!visible}
    onMouseEnter={() => setHovered(true)} onMouseLeave={() => setHovered(false)}
    onFocusCapture={() => setFocused(true)} onBlurCapture={() => setFocused(false)}
    className={cn(canvas?.tagName === "BODY" ? "fixed" : "absolute", "left-1/2 top-sm z-50 flex w-max max-w-[calc(100%_-_var(--sp-3xl))] -translate-x-1/2 items-center gap-lg rounded-control border border-border bg-card px-lg py-md text-label leading-label text-foreground shadow-lg", !visible && "pointer-events-none")}>
    <WarningCircleIcon size={16} className="shrink-0 text-warning" aria-hidden="true" />
    <span>{title}</span>
    <TooltipButton tooltip="Dismiss" type="button" variant="ghost" size="icon-compact" aria-label="Dismiss error notice" onClick={() => setVisible(false)}><XIcon size={16} aria-hidden="true" /></TooltipButton>
  </Toast>;
  return <>
    <Alert aria-label="Chat recovery" variant="warning" className="mx-auto mb-md grid w-full max-w-chat grid-cols-[1rem_minmax(0,1fr)] items-start gap-x-lg gap-y-xs rounded-none border-0 bg-transparent p-0 text-label leading-label text-muted-foreground sm:flex sm:flex-wrap sm:items-center">
      <WarningCircleIcon size={16} className="mt-xs shrink-0 text-warning sm:mt-0" aria-hidden="true" />
      <span className="min-w-0 break-words sm:flex-1">{children}</span>
      <div className="col-start-2 flex flex-wrap items-center gap-sm">{actions}</div>
    </Alert>
    {canvas ? createPortal(flash, canvas) : null}
  </>;
}
