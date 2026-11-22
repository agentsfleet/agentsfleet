import { useEffect, type RefObject } from "react";

export function useDemoPlayback(ref: RefObject<HTMLElement | null>) {
  useEffect(() => {
    const demo = ref.current!;
    const listeners = new AbortController();
    const signal = listeners.signal;
    const controls = demo.querySelector<HTMLElement>(".controls")!;
    const control = controls.querySelector("button")!;
    const meter = controls.querySelector<HTMLElement>(".meter span")!;
    const motionPreference = window.matchMedia(
      "(prefers-reduced-motion: reduce)",
    );
    const observeVisibility = "IntersectionObserver" in window;
    let inView = !observeVisibility;
    let paused = false;
    let complete = false;

    function syncPlayback() {
      const reduced = motionPreference.matches;
      demo.toggleAttribute(
        "data-playing",
        !reduced && inView && !document.hidden && !paused && !complete,
      );
      controls.hidden = reduced;
      control.textContent = complete
        ? "Replay demo"
        : paused
          ? "Resume demo"
          : "Pause demo";
    }

    function resetDemo() {
      paused = false;
      complete = false;
      demo.removeAttribute("data-motion");
      // Flush the prior animation before replaying on the same elements.
      void demo.offsetWidth;
      demo.toggleAttribute("data-motion", !motionPreference.matches);
      syncPlayback();
    }

    control.addEventListener(
      "click",
      () => {
        if (complete) {
          resetDemo();
        } else {
          paused = !paused;
          syncPlayback();
        }
      },
      { signal },
    );

    meter.addEventListener(
      "animationend",
      () => {
        complete = true;
        syncPlayback();
      },
      { signal },
    );

    let observer: IntersectionObserver | undefined;
    if (observeVisibility) {
      observer = new IntersectionObserver(
        (entries) => {
          const entry = entries.at(-1)!;
          inView = entry.isIntersecting && entry.intersectionRatio >= 0.45;
          syncPlayback();
        },
        { threshold: 0.45 },
      );
      observer.observe(demo.querySelector(".stage")!);
    }

    document.addEventListener("visibilitychange", syncPlayback, { signal });
    motionPreference.addEventListener("change", resetDemo, { signal });
    const small = matchMedia("(max-width:36rem)");
    const setup = demo.querySelector<HTMLDetailsElement>(".setup details")!;
    function sizeSummary() {
      setup.open = !small.matches;
    }
    demo
      .querySelectorAll(
        ".work-group > summary, .trace > summary, .setup summary",
      )
      .forEach((summary) => {
        summary.addEventListener(
          "click",
          () => {
            if (!complete && !motionPreference.matches) {
              paused = true;
              syncPlayback();
            }
          },
          { signal },
        );
      });
    sizeSummary();
    small.addEventListener("change", sizeSummary, { signal });
    resetDemo();
    return () => {
      listeners.abort();
      observer?.disconnect();
    };
  }, [ref]);
}
