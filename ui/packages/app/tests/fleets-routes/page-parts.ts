import type { ComponentProps, ReactNode } from "react";
import { Children, isValidElement } from "react";
import { ChatView } from "@/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/ChatView";

type Props = ComponentProps<typeof ChatView>;

/** Read the actual server-to-client payload; the browser owns deferred rendering. */
export function chatProps(page: ReactNode): Props {
  const props = findChat(page);
  if (!props) throw new Error("The fleet page did not supply a conversation");
  return props;
}

function findChat(node: ReactNode): Props | undefined {
  for (const child of Children.toArray(node)) {
    if (!isValidElement<{children?: ReactNode}>(child)) continue;
    if (child.type === ChatView && isValidElement<Props>(child)) return child.props;
    const nested = findChat(child.props.children);
    if (nested) return nested;
  }
  return undefined;
}
