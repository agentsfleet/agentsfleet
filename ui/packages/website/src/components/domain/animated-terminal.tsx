"use client";

import { useEffect, useRef, useState, type ReactNode } from "react";
import { tokenizeBash, type Token } from "./tokenize-bash";
import { useInView, usePrefersReducedMotion } from "./use-in-view";

/*
 * AnimatedTerminal — macOS-style chrome + typed demo of a shell script
 * (spec §5.8.4). Triggered on first scroll into view. Audio is
 * deliberately not wired (D4 scope decision — no .ogg sprite, no
 * useAudio hook, no enableSound prop). Syntax highlighting runs
 * through the pure `tokenizeBash` function exported for unit tests.
 *
 * SSR-safe: IntersectionObserver + matchMedia access is guarded in
 * hooks; the first render paints the chrome + prompt without touching
 * window APIs. `prefers-reduced-motion: reduce` collapses the phase
 * machine — all commands + outputs render instantly, cursor is solid.
 */

type Phase = "idle" | "typing" | "executing" | "outputting" | "pausing" | "done";

export interface AnimatedTerminalProps {
  commands: readonly string[];
  outputs?: Readonly<Record<number, readonly string[]>>;
  /** Per-command prompt override. Renders verbatim in place of the default
   * `username $ ` prompt — used when a command runs in a non-shell host
   * (e.g. a slash command typed into Claude Code, not zsh). */
  prompts?: Readonly<Record<number, string>>;
  username?: string;
  typingSpeed?: number;
  delayBetweenCommands?: number;
  initialDelay?: number;
  className?: string;
}

const TOKEN_CLASSNAME: Record<Token["type"], string> = {
  command: "text-success",
  flag: "text-info",
  string: "text-warning",
  number: "text-warning",
  path: "text-info/80",
  variable: "text-info",
  operator: "text-muted-foreground",
  comment: "text-muted-foreground italic",
  default: "text-muted-foreground",
};

export function AnimatedTerminal({
  commands,
  outputs = {},
  prompts = {},
  username = "you@usezombie",
  typingSpeed = 55,
  delayBetweenCommands = 700,
  initialDelay = 400,
  className,
}: AnimatedTerminalProps) {
  const rootRef = useRef<HTMLDivElement>(null);
  const inView = useInView(rootRef, { threshold: 0.2, once: true });
  const reducedMotion = usePrefersReducedMotion();

  // Lazy-initialize state so reduced-motion users land on a fully-printed
  // terminal on first render — no cascading-render effect needed, no
  // motion-path state ever entered. An empty `commands` array likewise
  // starts at "done".
  const [phase, setPhase] = useState<Phase>(() => {
    if (reducedMotion) return "done";
    if (commands.length === 0) return "done";
    return "idle";
  });
  const [commandIndex, setCommandIndex] = useState(0);
  const [typed, setTyped] = useState("");
  const [renderedLines, setRenderedLines] = useState<RenderedLine[]>(() =>
    reducedMotion ? buildInstantLines(commands, outputs, prompts) : [],
  );

  // Drive the phase machine once the element intersects.
  useEffect(() => {
    if (!inView || reducedMotion) return;
    if (phase !== "idle") return;
    const t = window.setTimeout(() => setPhase("typing"), initialDelay);
    return () => window.clearTimeout(t);
  }, [inView, phase, initialDelay, reducedMotion]);

  // Typing → executing → outputting → pausing → (next) → done.
  useEffect(() => {
    if (reducedMotion) return;
    if (phase === "typing") {
      // Guaranteed in-bounds: the "pausing" branch transitions to "done"
      // when the next index would be out of range, and the initial phase
      // falls back to "done" for empty `commands` arrays.
      const target = commands[commandIndex]!;
      if (typed.length < target.length) {
        const t = window.setTimeout(
          () => setTyped(target.slice(0, typed.length + 1)),
          typingSpeed,
        );
        return () => window.clearTimeout(t);
      }
      // Advance out of "typing" on a microtask — the React 19
      // react-hooks/set-state-in-effect rule rejects direct setState in
      // an effect body; a queued callback is the idiomatic way to
      // schedule the transition without cascading renders.
      const t = window.setTimeout(() => setPhase("executing"), 0);
      return () => window.clearTimeout(t);
    }
    if (phase === "executing") {
      const t = window.setTimeout(() => {
        const prompt: RenderedLine = {
          kind: "prompt",
          text: typed,
          prompt: prompts[commandIndex],
        };
        const out: RenderedLine[] = (outputs[commandIndex] ?? []).map((line) => ({
          kind: "output",
          text: line,
        }));
        setRenderedLines((prev) => [...prev, prompt, ...out]);
        setPhase("outputting");
      }, 180);
      return () => window.clearTimeout(t);
    }
    if (phase === "outputting") {
      const t = window.setTimeout(() => setPhase("pausing"), 0);
      return () => window.clearTimeout(t);
    }
    if (phase === "pausing") {
      const t = window.setTimeout(() => {
        if (commandIndex + 1 >= commands.length) {
          setPhase("done");
          return;
        }
        setCommandIndex(commandIndex + 1);
        setTyped("");
        setPhase("typing");
      }, delayBetweenCommands);
      return () => window.clearTimeout(t);
    }
  }, [phase, typed, commandIndex, commands, outputs, prompts, typingSpeed, delayBetweenCommands, reducedMotion]);

  return (
    <div
      ref={rootRef}
      role="region"
      aria-label="Interactive terminal demonstration"
      className={[
        "w-full overflow-hidden rounded-lg border border-border bg-card shadow-card",
        className ?? "",
      ].join(" ")}
    >
      <div className="flex items-center gap-2 border-b border-border px-4 py-2">
        <span className="h-3 w-3 rounded-full bg-destructive/70" aria-hidden="true" />
        <span className="h-3 w-3 rounded-full bg-warning/70" aria-hidden="true" />
        <span className="h-3 w-3 rounded-full bg-success/70" aria-hidden="true" />
        <span className="ml-3 font-mono text-xs text-muted-foreground">zsh</span>
      </div>

      <pre
        data-testid="terminal-body"
        className="min-h-[10rem] overflow-auto p-4 font-mono text-sm leading-6"
      >
        <code aria-live="polite">
          {renderedLines.map((line, i) => (
            <TerminalLine key={i} line={line} username={username} />
          ))}
          {phase !== "done" && !reducedMotion ? (
            <ActiveLine
              typed={typed}
              username={username}
              promptOverride={prompts[commandIndex]}
              showCursor
            />
          ) : null}
        </code>
      </pre>
    </div>
  );
}

type RenderedLine =
  | { kind: "prompt"; text: string; prompt?: string }
  | { kind: "output"; text: string };

function buildInstantLines(
  commands: readonly string[],
  outputs: Readonly<Record<number, readonly string[]>>,
  prompts: Readonly<Record<number, string>>,
): RenderedLine[] {
  const lines: RenderedLine[] = [];
  for (let i = 0; i < commands.length; i += 1) {
    lines.push({ kind: "prompt", text: commands[i]!, prompt: prompts[i] });
    for (const out of outputs[i] ?? []) {
      lines.push({ kind: "output", text: out });
    }
  }
  return lines;
}

function TerminalLine({
  line,
  username,
}: {
  line: RenderedLine;
  username: string;
}): ReactNode {
  if (line.kind === "output") {
    return (
      <div className="whitespace-pre-wrap text-muted-foreground">{line.text}</div>
    );
  }
  return (
    <div className="whitespace-pre-wrap">
      <Prompt username={username} override={line.prompt} />
      <SyntaxHighlighted tokens={tokenizeBash(line.text)} />
    </div>
  );
}

function ActiveLine({
  typed,
  username,
  promptOverride,
  showCursor,
}: {
  typed: string;
  username: string;
  promptOverride?: string;
  showCursor: boolean;
}): ReactNode {
  return (
    <div className="whitespace-pre-wrap">
      <Prompt username={username} override={promptOverride} />
      <SyntaxHighlighted tokens={tokenizeBash(typed)} />
      {showCursor ? (
        <span
          aria-hidden="true"
          className="ml-0.5 inline-block h-4 w-[0.5ch] animate-pulse bg-foreground align-[-2px]"
        />
      ) : null}
    </div>
  );
}

function Prompt({ username, override }: { username: string; override?: string }): ReactNode {
  if (override !== undefined) {
    return <span className="text-warning">{override} </span>;
  }
  return (
    <span>
      <span className="text-info">{username}</span>
      <span className="text-primary"> $ </span>
    </span>
  );
}

function SyntaxHighlighted({ tokens }: { tokens: Token[] }): ReactNode {
  return (
    <>
      {tokens.map((t, i) => (
        <span key={i} className={TOKEN_CLASSNAME[t.type]}>
          {t.text}
        </span>
      ))}
    </>
  );
}

export default AnimatedTerminal;
