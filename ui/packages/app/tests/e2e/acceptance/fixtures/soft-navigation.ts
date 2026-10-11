/**
 * Proof that a navigation stayed client-side.
 *
 * `page.goto` is a full document load: every React tree remounts and every
 * stream reopens, so a round-trip written with it can never catch a remount.
 * These helpers mark the document (a full load drops the mark) and count the
 * EventSources the page opens to one stream URL, per document.
 */
import type { Page } from "@playwright/test";

const DOCUMENT_MARK = "__softNavigationMark";
const STREAM_AUDIT = "__streamAudit";

export type StreamCount = { created: number; open: number };

const NO_STREAMS: StreamCount = { created: 0, open: 0 };

/** Counts, from the next document on, the EventSources opened to `streamUrl`. */
export async function auditStream(page: Page, streamUrl: string): Promise<void> {
  await page.addInitScript(
    ({ url, key }) => {
      const NativeEventSource = window.EventSource;
      const audit = { created: 0, open: 0 };
      class AuditedEventSource extends NativeEventSource {
        private counted: boolean;

        constructor(target: string | URL, init?: EventSourceInit) {
          super(target, init);
          this.counted = String(target).endsWith(url);
          if (this.counted) {
            audit.created += 1;
            audit.open += 1;
          }
        }

        close() {
          if (this.counted) {
            this.counted = false;
            audit.open -= 1;
          }
          super.close();
        }
      }
      window.EventSource = AuditedEventSource;
      Reflect.set(window, key, audit);
    },
    { url: streamUrl, key: STREAM_AUDIT },
  );
}

export async function streamCount(page: Page): Promise<StreamCount> {
  const count = await page.evaluate((key) => Reflect.get(window, key) as StreamCount | undefined, STREAM_AUDIT);
  return count ?? NO_STREAMS;
}

/** Marks the current document. A full page load starts a fresh, unmarked one. */
export async function markDocument(page: Page): Promise<void> {
  await page.evaluate((key) => Reflect.set(window, key, true), DOCUMENT_MARK);
}

export function documentIsMarked(page: Page): Promise<boolean> {
  return page.evaluate((key) => Reflect.get(window, key) === true, DOCUMENT_MARK);
}
