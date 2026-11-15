/** Bound browser-local first-paint deduplication to the visible timeline scale. */
export class RecentPaints {
  readonly #keys = new Set<string>();

  constructor(private readonly limit: number) {}

  has(key: string): boolean {
    return this.#keys.has(key);
  }

  add(key: string): void {
    if (this.#keys.has(key)) return;
    if (this.#keys.size === this.limit) {
      // The set is nonempty whenever the positive limit is reached.
      this.#keys.delete(this.#keys.values().next().value as string);
    }
    this.#keys.add(key);
  }
}
