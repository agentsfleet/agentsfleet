"use client";

import type { SyntaxHighlighterProps } from "@assistant-ui/react-markdown";
import { CodeHeader } from "./CodeHeader";

/** Official Markdown slot; the SDK still owns fences, Pre, Code and streaming. */
export function CodeBlock({node, language, code, components: {Pre, Code}}: SyntaxHighlighterProps) {
  return <div data-code-block className="overflow-hidden rounded-md border border-border bg-muted">
    <CodeHeader language={language} code={code} />
    <Pre node={node}><Code className={language ? `language-${language}` : undefined}>{code}</Code></Pre>
  </div>;
}
