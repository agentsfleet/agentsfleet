type RefreshListener = () => void;

class OnboardingRefreshBus {
  readonly #listenersByWorkspace = new Map<string, Set<RefreshListener>>();

  request(workspaceId: string) {
    const listeners = this.#listenersByWorkspace.get(workspaceId);
    if (!listeners) return;
    for (const listener of [...listeners]) listener();
  }

  subscribe(workspaceId: string, listener: RefreshListener) {
    const listeners = this.#listenersByWorkspace.get(workspaceId) ?? new Set<RefreshListener>();
    listeners.add(listener);
    this.#listenersByWorkspace.set(workspaceId, listeners);

    return () => {
      listeners.delete(listener);
      if (listeners.size === 0) this.#listenersByWorkspace.delete(workspaceId);
    };
  }
}

const onboardingRefreshBus = new OnboardingRefreshBus();

export function requestOnboardingRefresh(workspaceId: string) {
  onboardingRefreshBus.request(workspaceId);
}

export function subscribeOnboardingRefresh(workspaceId: string, listener: RefreshListener) {
  return onboardingRefreshBus.subscribe(workspaceId, listener);
}
