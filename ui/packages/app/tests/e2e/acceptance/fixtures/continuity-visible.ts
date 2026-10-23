import type { Page } from "@playwright/test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

/** Match the app's Markdown dialect before comparing displayed and durable text. */
export async function visibleReply(page: Page, text: string): Promise<string> {
  const html = renderToStaticMarkup(createElement(ReactMarkdown, {remarkPlugins: [remarkGfm]}, text));
  return page.evaluate(markup => new DOMParser().parseFromString(markup, "text/html").body.textContent!, html);
}
