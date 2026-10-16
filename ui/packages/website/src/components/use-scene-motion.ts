import type { RefObject } from "react";
import { useEffect, useState } from "react";
import { STORY_FRAME_MS } from "../lib/chat-story";
import { useMotionEnvironment } from "./use-motion-environment";

export function useSceneMotion(ref: RefObject<HTMLElement | null>) {
  const [elapsed, setElapsed] = useState(0);
  const [playing, setPlaying] = useState(true);
  const [complete, setComplete] = useState(false);
  const [resumeGeneration, setResumeGeneration] = useState(0);
  const { active, staticMode } = useMotionEnvironment(ref);
  const running = playing && active && !complete;
  useEffect(() => {
    if (!running) return;
    let previous = performance.now();
    const timer = window.setInterval(() => {
      const now = performance.now();
      const delta = now - previous;
      previous = now;
      setElapsed((current) => current + delta);
    }, STORY_FRAME_MS);
    return () => window.clearInterval(timer);
  }, [running]);
  const pause = () => setPlaying(false);
  const readAll = () => { setComplete(true); pause(); };
  const toggle = () => {
    if (complete) { setComplete(false); setElapsed(0); }
    if (!playing) setResumeGeneration((current) => current + 1);
    setPlaying((current) => !current);
  };
  return { elapsed, playing, running, staticMode, complete: complete || staticMode, resumeGeneration, pause, readAll, toggle };
}
