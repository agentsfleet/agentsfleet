"use client";

import { createContext, memo, useContext, type ReactNode } from "react";
import { MarkdownTextPrimitive, memoizeMarkdownComponents } from "@assistant-ui/react-markdown";
import { useAuiState } from "@assistant-ui/react";
import Markdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { MessageExpansion } from "./MessageExpansion";

/**
 * A fleet's reply, as markdown.
 *
 * Assistant messages use the library's streaming text-part renderer. Standalone
 * text, including user messages, uses the same markdown engine and elements.
 * Raw HTML is never parsed: models and users are untrusted authors.
 *
 * Every element maps to a design-system token rather than to prose defaults, so
 * a reply sits in the same type scale as the surface around it.
 */

/** Set by `pre`, so a fenced block's `code` does not also render as a chip. */
const Fenced = createContext(false);

// Module scope, so a finished block's props never change identity under it.
const REMARK_PLUGINS = [remarkGfm];

const COMPONENTS = {
  p: ({ children }) => <p className="leading-conversation">{children}</p>,
  ul: ({ children }) => <ul className="list-disc space-y-xs pl-lg">{children}</ul>,
  ol: ({ children }) => <ol className="list-decimal space-y-xs pl-lg">{children}</ol>,
  li: ({ children }) => <li className="leading-conversation">{children}</li>,
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
} satisfies Components;

// Match the approved conversation type in setup and saved fleet chat.
const BODY_CLASS = "space-y-lg font-sans text-conversation leading-conversation text-foreground";

const REPLY_COMPONENTS = memoizeMarkdownComponents(COMPONENTS);

/** Keep the library's text-part renderer mounted through streaming and completion. */
export const FleetReplyMarkdown = memo(function FleetReplyMarkdown() {
  const length = useAuiState(state => state.message.content.reduce((total, part) => total + (part.type === "text" ? part.text.length : 0), 0));
  const running = useAuiState(state => state.message.status?.type === "running");
  return <MessageExpansion length={length} running={running}>{display => <MarkdownTextPrimitive className={BODY_CLASS} components={REPLY_COMPONENTS}
    remarkPlugins={REMARK_PLUGINS} preprocess={display} smooth defer />}</MessageExpansion>;
});

export const FleetMarkdown = memo(function FleetMarkdown({ children, expandable = false }: { children: string; expandable?: boolean }) {
  return (
    <MessageExpansion length={expandable ? children.length : 0}>{display =>
    <div className={BODY_CLASS}>
      <Markdown components={COMPONENTS} remarkPlugins={REMARK_PLUGINS}>
        {display(children)}
      </Markdown>
    </div>
    }</MessageExpansion>
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
