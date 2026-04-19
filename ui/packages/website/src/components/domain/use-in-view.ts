import { useEffect, useRef, useState, type RefObject } from "react";

/*
 * useInView — minimal IntersectionObserver hook used by <AnimatedTerminal />
 * to gate the typing animation on first scroll-into-view. `once` keeps the
 * fired state sticky so the animation does not restart if the user scrolls
 * away and back.
 *
 * SSR-safe: `IntersectionObserver` is only touched inside `useEffect`, which
 * runs post-mount. If the API is missing (very old browsers), the lazy
 * state initializer seeds `true` so content is always visible.
 */
export function useInView<T extends Element>(
  ref: RefObject<T | null>,
  options: { threshold?: number; once?: boolean } = {},
): boolean {
  const { threshold = 0.1, once = true } = options;
  const [inView, setInView] = useState(
    () => typeof window !== "undefined" && typeof IntersectionObserver === "undefined",
  );
  const firedRef = useRef(false);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (typeof IntersectionObserver === "undefined") return;

    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            if (once && firedRef.current) return;
            firedRef.current = true;
            setInView(true);
            if (once) observer.disconnect();
          } else if (!once) {
            setInView(false);
          }
        }
      },
      { threshold },
    );

    observer.observe(el);
    return () => observer.disconnect();
  }, [ref, threshold, once]);

  return inView;
}

/** Read `prefers-reduced-motion: reduce` once on mount — never auto-toggles
 * mid-session because changing preference mid-animation is jarring. The
 * lazy `useState` initializer reads `matchMedia` synchronously during
 * render on the client (and returns `false` during SSR where `window`
 * is undefined), so there is no cascading-render post-mount. */
export function usePrefersReducedMotion(): boolean {
  const [reduced] = useState(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
      return false;
    }
    return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  });
  return reduced;
}
