import type { RefObject } from "react";
import { useEffect, useState } from "react";
import type { StoryFrame } from "../lib/chat-story";

const FOLLOW_DISTANCE = 24;

export function useChatScroll(ref: RefObject<HTMLDivElement | null>, frame: StoryFrame, running: boolean) {
  const [following, setFollowing] = useState(true);
  useEffect(() => {
    if (running && following && ref.current) ref.current.scrollTop = ref.current.scrollHeight;
  }, [ref, frame, running, following]);
  const onScroll = () => {
    const element = ref.current;
    if (running && element) setFollowing(element.scrollHeight - element.clientHeight - element.scrollTop <= FOLLOW_DISTANCE);
  };
  const jump = () => {
    setFollowing(true);
    ref.current?.scrollTo({ top: ref.current.scrollHeight, behavior: "smooth" });
  };
  return { following, onScroll, jump };
}
