"use client";

import { useEffect } from "react";

/*
 * Publishes the width the canvas actually reserves for its scrollbar.
 *
 * The canvas scrolls and reserves a gutter on both edges; the header does not
 * scroll, so it adds that same width to land its trailing cluster on the
 * canvas's content edge. How wide that is depends on the platform and on the
 * route: a classic scrollbar takes layout width, an overlay one takes none and
 * `scrollbar-gutter` then reserves nothing, and a route whose canvas is
 * `overflow: hidden` reserves nothing either.
 *
 * The stylesheet's 6px was measured on one desktop. Everywhere overlay
 * scrollbars are the default the canvas would sit at the gutter while the
 * header sat 6px further in — the same misalignment, mirrored. So the number is
 * measured here and written back to the token the header already reads, and the
 * stylesheet's value is only the value used before this mounts.
 */
const CANVAS_SELECTOR = "main.app-dashboard-canvas";
const SCROLLBAR_VAR = "--app-scrollbar";
// `scrollbar-gutter: stable both-edges` reserves the same width on each edge,
// and offsetWidth - clientWidth is the pair.
const RESERVED_EDGES = 2;

/** The reserved width per edge, in pixels, for a canvas element. */
export function reservedGutterWidth(canvas: HTMLElement): number {
  const both = canvas.offsetWidth - canvas.clientWidth;
  return both > 0 ? both / RESERVED_EDGES : 0;
}

export function CanvasScrollbarProbe() {
  useEffect(() => {
    const canvas = document.querySelector<HTMLElement>(CANVAS_SELECTOR);
    if (!canvas) return;

    const publish = () => {
      const reserved = reservedGutterWidth(canvas);
      document.documentElement.style.setProperty(SCROLLBAR_VAR, `${reserved}px`);
    };

    publish();
    // The reservation appears and disappears with the content: a route that
    // stops overflowing gives the width back, and the header has to follow.
    const observer = new ResizeObserver(publish);
    observer.observe(canvas);
    return () => observer.disconnect();
  }, []);

  return null;
}

export default CanvasScrollbarProbe;
