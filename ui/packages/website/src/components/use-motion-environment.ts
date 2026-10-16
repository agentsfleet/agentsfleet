import type { RefObject } from "react";
import { useEffect, useState } from "react";

const STATIC_QUERY = "(prefers-reduced-motion: reduce)";

export function useMotionEnvironment(ref: RefObject<Element | null>) {
  const [visible, setVisible] = useState(false);
  const [hidden, setHidden] = useState(false);
  const [staticMode, setStaticMode] = useState(true);
  useEffect(() => {
    const media = window.matchMedia(STATIC_QUERY);
    const updateMedia = () => setStaticMode(media.matches);
    const updateHidden = () => setHidden(document.hidden);
    updateMedia(); updateHidden();
    media.addEventListener("change", updateMedia);
    document.addEventListener("visibilitychange", updateHidden);
    const observer = new IntersectionObserver(([entry]) => setVisible(Boolean(entry?.isIntersecting)), { threshold: 0.25 });
    if (ref.current) observer.observe(ref.current);
    return () => { observer.disconnect(); media.removeEventListener("change", updateMedia); document.removeEventListener("visibilitychange", updateHidden); };
  }, [ref]);
  return { active: visible && !hidden && !staticMode, staticMode };
}
