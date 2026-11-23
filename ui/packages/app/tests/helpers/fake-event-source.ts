import type { LiveFrame } from "@/lib/api/turns";
import { FRAME_KIND } from "@/lib/api/turns-types";
import { ERROR_CODE } from "@/lib/errors";

// The one EventSource double for every Server-Sent Events test.
//
// It dispatches exactly as a browser does. The daemon names every frame with
// its payload kind (`event: chunk`, `event: turn_complete` — written by
// sse_frame.writeHead), and a NAMED frame reaches ONLY the listeners
// registered under that name — never `onmessage`. A kind nobody subscribed to
// is dropped silently while the connection still reports itself live, so a
// client that skips addEventListener looks healthy and renders nothing.
//
// One copy on purpose. A double that is free to model a friendlier server than
// the real one turns a green suite into no evidence at all: a client wired only
// to `onmessage` passes against a fake that delivers everything there, and
// fails against a browser.
export class FakeEventSource {
  static instances: FakeEventSource[] = [];
  readonly url: string;
  onopen: ((this: EventSource, ev: Event) => unknown) | null = null;
  onmessage: ((this: EventSource, ev: MessageEvent) => unknown) | null = null;
  onerror: ((this: EventSource, ev: Event) => unknown) | null = null;
  closed = false;
  readonly listeners = new Map<string, Set<(ev: MessageEvent) => void>>();
  readonly startedStreams = new Set<string>();
  readonly nextStreamSeq = new Map<string, number>();

  constructor(url: string) {
    this.url = url;
    FakeEventSource.instances.push(this);
  }

  // Become the global `EventSource` the module under test constructs, with no
  // connections carried over from the previous test.
  static install(): void {
    FakeEventSource.instances = [];
    (globalThis as unknown as { EventSource: unknown }).EventSource =
      FakeEventSource;
  }

  static uninstall(): void {
    delete (globalThis as { EventSource?: unknown }).EventSource;
  }

  close(): void {
    this.closed = true;
  }

  addEventListener(name: string, fn: (ev: MessageEvent) => void): void {
    const named =
      this.listeners.get(name) ?? new Set<(ev: MessageEvent) => void>();
    named.add(fn);
    this.listeners.set(name, named);
  }

  removeEventListener(name: string, fn: (ev: MessageEvent) => void): void {
    this.listeners.get(name)?.delete(fn);
  }

  // A named frame — the shape the daemon actually writes for every payload.
  emit(frame: LiveFrame): void {
    const named = this.listeners.get(frame.kind);
    if (!named) return;
    // Most fixtures describe a run from its first chunk. Explicit markers
    // exercise late subscribers, gaps, and reassignment on the same event.
    // Fixtures default to the daemon's first attempt; explicit missing fields stay missing.
    let wire: LiveFrame = "turn_id" in frame && !("fencing_token" in frame)
      ? {...frame, fencing_token: "1"} : frame;
    if (frame.kind === FRAME_KIND.CHUNK) {
      const fence = "fencing_token" in wire ? wire.fencing_token : undefined;
      const stream = `${frame.turn_id}:${fence}`;
      const seq = frame.stream_seq ?? this.nextStreamSeq.get(stream) ?? 0;
      wire = {
        ...frame, fencing_token: fence,
        stream_start: frame.stream_start ?? !this.startedStreams.has(stream),
        stream_contiguous: frame.stream_contiguous ?? true,
        stream_seq: seq,
      };
      this.startedStreams.add(stream);
      this.nextStreamSeq.set(stream, seq + 1);
    }
    this.emitNamed(wire.kind, JSON.stringify(wire));
  }

  emitNamed(name: string, data: string): void {
    const ev = new MessageEvent(name, {data});
    for (const fn of this.listeners.get(name) ?? []) fn(ev);
  }

  // Raw bytes on the daemon's no-kind fallback channel, which arrives as
  // `event: message` and is therefore the one shape `onmessage` receives.
  // Takes the payload verbatim so a test can feed it something that is not
  // JSON at all, which is exactly what the parse guards exist for.
  emitRaw(data: string): void {
    this.onmessage?.call(this as unknown as EventSource, {
      data,
    } as MessageEvent);
  }

  open(): void {
    this.onopen?.call(this as unknown as EventSource, {} as Event);
  }

  heartbeat(): void {
    const ev = { data: "" } as MessageEvent;
    for (const fn of this.listeners.get("heartbeat") ?? []) fn(ev);
  }

  // The daemon's last frame to a caller who lost access, as
  // rustd/crates/daemon/afd_sse/src/frame/mod.rs `Frame::access_revoked` writes it.
  revokeAccess(): void {
    const ev = { data: JSON.stringify({ kind: FRAME_KIND.ACCESS_REVOKED, error_code: ERROR_CODE.AUTH_FORBIDDEN }) } as MessageEvent;
    for (const fn of this.listeners.get(FRAME_KIND.ACCESS_REVOKED) ?? []) fn(ev);
  }

  fail(): void {
    this.onerror?.call(this as unknown as EventSource, {} as Event);
  }
}
