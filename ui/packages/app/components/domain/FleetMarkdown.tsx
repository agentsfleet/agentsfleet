"use client";

import { createContext, memo, useContext, type ReactNode } from "react";
import Markdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";

/**
 * A fleet's reply, as markdown.
 *
 * Models write markdown whether or not anything renders it, so the chat used to
 * print `- **A repo + PR number**` verbatim and collapse every newline into a
 * space — a numbered list arrived as one long paragraph. `react-markdown` is
 * the engine here (it is what `@assistant-ui/react-markdown` wraps; that
 * package's primitive reads the message's own text part, and the fleet's reply
 * lives in a separate field it cannot see). Raw HTML is never parsed, which is
 * the posture worth having when the author is a model.
 *
 * Every element maps to a design-system token rather than to prose defaults, so
 * a reply sits in the same type scale as the surface around it.
 */

/** Set by `pre`, so a fenced block's `code` does not also render as a chip. */
const Fenced = createContext(false);

const COMPONENTS: Components = {
  p: ({ children }) => <p className="leading-reading">{children}</p>,
  ul: ({ children }) => <ul className="list-disc space-y-xs pl-lg">{children}</ul>,
  ol: ({ children }) => <ol className="list-decimal space-y-xs pl-lg">{children}</ol>,
  li: ({ children }) => <li className="leading-reading">{children}</li>,
  strong: ({ children }) => <strong className="font-medium text-foreground">{children}</strong>,
  em: ({ children }) => <em className="italic">{children}</em>,
  h1: ({ children }) => <Heading>{children}</Heading>,
  h2: ({ children }) => <Heading>{children}</Heading>,
  h3: ({ children }) => <Heading>{children}</Heading>,
  h4: ({ children }) => <Heading>{children}</Heading>,
  h5: ({ children }) => <Heading>{children}</Heading>,
  h6: ({ children }) => <Heading>{children}</Heading>,
  blockquote: ({ children }) => (
    <blockquote className="border-l-2 border-border pl-md text-muted-foreground">
      {children}
    </blockquote>
  ),
  hr: () => <hr className="border-border" />,
  // A model is an untrusted author: every link it writes opens away from the
  // dashboard and cannot reach back through `window.opener`.
  a: ({ children, href }) => (
    <a
      href={href}
      target="_blank"
      rel="noreferrer noopener"
      className="underline decoration-border-strong underline-offset-2 hover:decoration-foreground"
    >
      {children}
    </a>
  ),
  pre: ({ children }) => (
    <Fenced value={true}>
      <pre className="overflow-x-auto rounded-md bg-muted p-md font-mono text-mono leading-mono">
        {children}
      </pre>
    </Fenced>
  ),
  code: ({ children }) => <Code>{children}</Code>,
  // No design-system primitive fits a markdown table: `DataTable` is a sortable,
  // paginated data grid, and this is prose the model wrote. Kept raw, and kept
  // scrollable so a wide table never widens the conversation column.
  table: ({ children }) => (
    <div className="overflow-x-auto">
      <table className="w-full text-body-sm">{children}</table>
    </div>
  ),
  th: ({ children }) => (
    <th className="border border-border px-sm py-xs text-left font-medium">{children}</th>
  ),
  td: ({ children }) => <td className="border border-border px-sm py-xs">{children}</td>,
};

/*
 * The transcript's own type: 16px, warm, and a shade lighter than it is drawn.
 *
 * `body` (15px) is the app's default for reading AND controls, and everywhere
 * else that dual duty is right: labels, table cells, form text, where compact
 * is a virtue. The transcript is the one surface that is sustained prose — the
 * fleet's actual output, read end to end — and it was set at the same size as
 * a dropdown label.
 *
 * 16px, not 18px, and not 15px. Measured 2026-09-13: Claude and ChatGPT both
 * set their transcripts at 16/400. Their chrome runs ~14px, so 16 buys them
 * one clear step; ours runs 15px, so 16 buys the same separation without
 * reaching `body-lg`, which belongs to website introductions. `--fs-reading`
 * exists for this one role — it is not the return of `text-base`, which was
 * removed the same week precisely because it had no role.
 *
 * `wght 380` rather than the nominal 400. Light-on-dark text blooms: the
 * strokes spread optically and read heavier than they measure, which is why
 * Claude runs its transcript at `wght 360` rather than 400. Instrument Sans is
 * already a variable face here (`instrument-sans-latin-wght-normal.woff2`), so
 * this costs no bytes — the axis ships whether or not we use it.
 *
 * `text-text-chat` is a warm off-white scoped to this surface alone. A cool
 * white glares against the graphite canvas at reading length; `--text` stays
 * cool for the rest of the product, which is interface rather than prose.
 */
export const FleetMarkdown = memo(function FleetMarkdown({ children }: { children: string }) {
  return (
    <div className="space-y-md font-sans text-reading leading-reading text-text-chat [font-variation-settings:'wght'_380]">
      <Markdown components={COMPONENTS} remarkPlugins={[remarkGfm]}>
        {children}
      </Markdown>
    </div>
  );
});

// One size for every level. A reply is a paragraph in a conversation, not a
// document, so an `###` inside it is a label — nesting six type scales into a
// chat bubble buys hierarchy nobody is navigating.
function Heading({ children }: { children: ReactNode }) {
  return <p className="text-label font-medium text-foreground">{children}</p>;
}

function Code({ children }: { children: ReactNode }) {
  if (useContext(Fenced)) return <code>{children}</code>;
  return (
    <code className="rounded-sm bg-muted px-xs font-mono text-mono leading-mono">{children}</code>
  );
}
