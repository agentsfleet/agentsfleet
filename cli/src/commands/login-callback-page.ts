export const PAGE_STATUS = { success: "success", denied: "denied", failed: "failed" } as const;
export type CallbackPageStatus = (typeof PAGE_STATUS)[keyof typeof PAGE_STATUS];

const PAGE_COPY = {
  [PAGE_STATUS.success]: ["Terminal authorized", "You can return to your terminal."],
  [PAGE_STATUS.denied]: ["Authorization denied", "Your terminal has not been signed in. You can close this page."],
  [PAGE_STATUS.failed]: ["Sign-in did not complete", "Return to your terminal for details, then start a new sign-in."],
} as const;

export function callbackPage(status: CallbackPageStatus): Response {
  const [title, message] = PAGE_COPY[status];
  const html = `<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>${title} · agentsfleet</title><style>html{color-scheme:light dark;font-family:system-ui,sans-serif}body{margin:0;min-height:100vh;display:grid;place-items:center}main{max-width:32rem;padding:2rem}h1{font-size:1.5rem;font-weight:600}p{line-height:1.6}</style><main><p>agentsfleet</p><h1>${title}</h1><p>${message}</p></main></html>`;
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
