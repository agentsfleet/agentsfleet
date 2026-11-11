"use client";

import type { ReactNode } from "react";
import { createContext, useCallback, useContext, useEffect, useRef, useState } from "react";
import Link from "next/link";
import { CheckCircleIcon, XIcon } from "@phosphor-icons/react/ssr";
import { Button, Toast, cn } from "@agentsfleet/design-system";
import { useCurrentUser } from "@/lib/auth/client";

type Notice = {outcome: "archived" | "deleted" | "absent"; historyHref: string};
type VisibleNotice = Notice & {id: number; visible: boolean};
const NOTICE_MS = 8_000;
const NoticeContext = createContext<(notice: Notice) => void>(() => {});

export const useFleetNotice = () => useContext(NoticeContext);

/** The shell owns confirmations so navigation cannot erase them. */
export function FleetNoticeProvider({children}: {children: ReactNode}) {
  const {userId} = useCurrentUser();
  const [notice, setNotice] = useState<VisibleNotice | null>(null);
  const nextId = useRef(0);
  const show = useCallback((value: Notice) => {
    nextId.current += 1;
    setNotice({...value, id: nextId.current, visible: true});
  }, []);
  const dismiss = useCallback(() => setNotice(previous => previous ? {...previous, visible: false} : null), []);
  useEffect(() => setNotice(null), [userId]);
  return <NoticeContext value={show}>{children}<FleetNotice notice={notice} dismiss={dismiss} /></NoticeContext>;
}

function FleetNotice({notice, dismiss}: {notice: VisibleNotice | null; dismiss: () => void}) {
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  const remaining = useRef(NOTICE_MS);
  useEffect(() => {remaining.current = NOTICE_MS;}, [notice?.id]);
  useEffect(() => {
    if (!notice?.visible || hovered || focused) return;
    const started = Date.now();
    const timer = setTimeout(dismiss, remaining.current);
    return () => {clearTimeout(timer); remaining.current = Math.max(0, remaining.current - (Date.now() - started));};
  }, [notice?.id, notice?.visible, hovered, focused, dismiss]);
  const message = notice?.outcome === "archived" ? "Fleet archived" : notice?.outcome === "deleted" ? "Fleet permanently deleted" : "Fleet is no longer available";
  return <Toast visible={notice?.visible ?? false} severity="success" aria-live="polite" inert={!notice?.visible} data-testid="fleet-notice"
    onMouseEnter={() => setHovered(true)} onMouseLeave={() => setHovered(false)}
    onFocusCapture={() => setFocused(true)} onBlurCapture={event => {if (!event.currentTarget.contains(event.relatedTarget)) setFocused(false);}}
    className={cn("fixed left-1/2 top-lg z-50 flex max-w-[calc(100vw-2rem)] -translate-x-1/2 items-center gap-lg rounded-md border border-border bg-card px-xl py-md text-label leading-label text-foreground shadow-lg", !notice?.visible && "pointer-events-none")}>
    <CheckCircleIcon size={16} className="shrink-0 text-success" aria-hidden="true" />
    <span>{message}</span>
    {notice?.outcome === "archived" ? <Link href={notice.historyHref} className="whitespace-nowrap underline underline-offset-4">View history</Link> : null}
    <Button variant="ghost" size="icon-compact" aria-label="Dismiss notification" onClick={dismiss}><XIcon size={16} aria-hidden="true" /></Button>
  </Toast>;
}
