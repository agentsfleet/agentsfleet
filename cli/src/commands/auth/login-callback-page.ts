export const PAGE_STATUS = { success: "success", denied: "denied", failed: "failed" } as const;
export type CallbackPageStatus = (typeof PAGE_STATUS)[keyof typeof PAGE_STATUS];

const PAGE_COPY = {
  [PAGE_STATUS.success]: ["Terminal authorized", "You can return to your terminal."],
  [PAGE_STATUS.denied]: ["Authorization denied", "Your terminal has not been signed in. You can close this page."],
  [PAGE_STATUS.failed]: ["Sign-in did not complete", "Return to your terminal for details, then start a new sign-in."],
} as const;

/**
 * The brand mark's path, copied from `ui/packages/design-system/src/assets/logo-dark.svg`.
 * Inline because the page loads nothing (`default-src 'none'`). Copy it again when the logo changes.
 */
export const BRAND_MARK_PATH =
  "M12 2v6m0 8v6M2 12h6m8 0h6M4.93 4.93l4.24 4.24m5.66 5.66 4.24 4.24M4.93 19.07l4.24-4.24m5.66-5.66 4.24-4.24";

const BRAND_MARK = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" aria-hidden="true"><path d="${BRAND_MARK_PATH}"/><circle cx="12" cy="12" r="1.5" fill="currentColor" stroke="none"/></svg>`;

const STATUS_ICON = {
  [PAGE_STATUS.success]: '<path d="M5 12.5l4.5 4.5L19 7.5"/>',
  [PAGE_STATUS.denied]: '<path d="M7 7l10 10M17 7 7 17"/>',
  [PAGE_STATUS.failed]: '<path d="M12 7v6m0 4h.01"/>',
} as const;

// The design system's brand colours: accent and text for light, then for dark.
const STYLE = [
  ":root{color-scheme:light dark;--bg:#faf9f5;--card:#fff;--border:#e4e1d8;--text:#14251f;--muted:#5b6b64;--accent:#06695e;--danger:#a4332b}",
  "@media (prefers-color-scheme:dark){:root{--bg:#121212;--card:#1a1a1a;--border:#2c2c2c;--text:#f2f0e9;--muted:#a3a39b;--accent:#b8f0ce;--danger:#f2a49c}}",
  "*{box-sizing:border-box}",
  'body{margin:0;min-height:100vh;display:grid;place-items:center;background:var(--bg);color:var(--text);font-family:"Instrument Sans",system-ui,sans-serif}',
  "main{width:min(26rem,calc(100vw - 2rem));padding:2rem;border:1px solid var(--border);border-radius:12px;background:var(--card);text-align:center}",
  ".brand{display:inline-flex;align-items:center;gap:.5rem;font-size:1.125rem;font-weight:500;letter-spacing:-.01em}",
  ".brand svg{width:28px;height:28px;color:var(--accent)}",
  ".status{margin:1.5rem auto 1rem;width:48px;height:48px;border:1.5px solid currentColor;border-radius:50%;display:grid;place-items:center}",
  ".status svg{width:24px;height:24px}",
  ".success{color:var(--accent)}.denied,.failed{color:var(--danger)}",
  "h1{margin:0 0 .5rem;font-size:1.375rem;font-weight:600}",
  "p{margin:0;line-height:1.6;color:var(--muted)}",
].join("");

export function callbackPage(status: CallbackPageStatus): Response {
  const [title, message] = PAGE_COPY[status];
  const icon = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${STATUS_ICON[status]}</svg>`;
  const html = `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>${title} · agentsfleet</title><style>${STYLE}</style></head><body><main><div class="brand">${BRAND_MARK}<span>agentsfleet</span></div><div class="status ${status}">${icon}</div><h1>${title}</h1><p>${message}</p></main></body></html>`;
  return new Response(html, {
    headers: {
      "Content-Type": "text/html; charset=utf-8",
      "Cache-Control": "no-store",
      "Referrer-Policy": "no-referrer",
      "X-Content-Type-Options": "nosniff",
      "Content-Security-Policy": "default-src 'none'; style-src 'unsafe-inline'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
      Connection: "close",
    },
  });
}
