"use client";

import type { ReactNode } from "react";
import { createContext, useCallback, useContext, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import Link from "next/link";
import {createPortal} from "react-dom";
import { ArchiveIcon, TrashIcon, CheckCircleIcon, XIcon } from "@phosphor-icons/react/ssr";
import { Button, Toast, cn } from "@agentsfleet/design-system";
import { useCurrentUser } from "@/lib/auth/client";

type Notice = {outcome: "archived" | "deleted" | "absent"; historyHref: string};
type VisibleNotice = Notice & {id: number; visible: boolean};
type OwnedNotice = VisibleNotice & {owner: {readonly userId: string | null}};
const NOTICE_MS = 8_000;
const NoticeContext = createContext<(notice: Notice) => void>(() => {});

export const useFleetNotice = () => useContext(NoticeContext);

/** The shell owns confirmations so navigation cannot erase them. */
export function FleetNoticeProvider({children}: {children: ReactNode}) {
  const {userId} = useCurrentUser();
  const owner = useMemo(() => ({userId}), [userId]);
  const activeOwner = useRef(owner);
  const [notice, setNotice] = useState<OwnedNotice | null>(null);
  const nextId = useRef(0);
  useLayoutEffect(() => {activeOwner.current = owner; setNotice(null);}, [owner]);
  const write = useCallback((value: OwnedNotice) => {
    if (activeOwner.current !== owner) return;
    setNotice(value);
  }, [owner]);
  const show = useCallback((value: Notice) => {
    nextId.current += 1;
    write({...value, owner, id: nextId.current, visible: true});
  }, [owner, write]);
  const current = notice?.owner === owner ? notice : null;
  return <NoticeContext value={show}>{children}{current ? <FleetNotice notice={current} dismiss={() => write({...current, visible: false})} /> : null}</NoticeContext>;
}

function FleetNotice({notice, dismiss}: {notice: VisibleNotice; dismiss: () => void}) {
  const [canvas, setCanvas] = useState<Element | null>(null);
  useLayoutEffect(() => {setCanvas(document.querySelector(".app-dashboard-canvas"));}, []);
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  const remaining = useRef(NOTICE_MS);
  useEffect(() => {remaining.current = NOTICE_MS;}, [notice.id]);
  useEffect(() => {
    if (!notice.visible || hovered || focused) return;
    const started = Date.now();
    const timer = setTimeout(dismiss, remaining.current);
    return () => {clearTimeout(timer); remaining.current = Math.max(0, remaining.current - (Date.now() - started));};
  }, [notice.id, notice.visible, hovered, focused, dismiss]);
  const message = notice.outcome === "archived" ? "Fleet archived" : notice.outcome === "deleted" ? "Fleet permanently deleted" : "Fleet is no longer available";
  const Icon = notice.outcome === "archived" ? ArchiveIcon : notice.outcome === "deleted" ? TrashIcon : CheckCircleIcon;
  const content = <Toast visible={notice.visible} severity="success" aria-live="polite" inert={!notice.visible} data-testid="fleet-notice"
    onMouseEnter={() => setHovered(true)} onMouseLeave={() => setHovered(false)}
    onFocusCapture={() => setFocused(true)} onBlurCapture={event => {if (!event.currentTarget.contains(event.relatedTarget)) setFocused(false);}}
    className={cn(canvas ? "absolute" : "fixed", "left-1/2 top-xs z-50 flex w-max max-w-[calc(100vw-2rem)] -translate-x-1/2 items-center gap-lg rounded-md border border-border bg-card px-xl py-md text-label leading-label text-foreground shadow-lg", !notice.visible && "pointer-events-none")}>
    <Icon size={16} className="shrink-0 text-success" aria-hidden="true" />
    <span>{message}</span>
    {notice.outcome === "archived" ? <Link href={notice.historyHref} className="whitespace-nowrap underline underline-offset-4">View history</Link> : null}
    <Button variant="ghost" size="icon-compact" aria-label="Dismiss notification" onClick={dismiss}><XIcon size={16} aria-hidden="true" /></Button>
  </Toast>;
  return canvas ? createPortal(content, canvas) : content;
}
