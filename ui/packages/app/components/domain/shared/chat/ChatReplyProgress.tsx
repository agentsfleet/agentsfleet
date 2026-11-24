export function ChatReplyProgress({label}: {label: string}) {
  return <output aria-label={label} className="flex h-lh items-center text-text-subtle">
    <span className="chat-reply-dots" aria-hidden="true"><span /><span /><span /></span>
    <span className="sr-only">{label}</span>
  </output>;
}
