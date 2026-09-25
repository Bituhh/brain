// Sliding-window per-character prediction accuracy -- VAL-4's stated
// metric, factored out so both the network's own evaluation and the
// trigram baseline (Requirement 13.3) use the identical measurement,
// keeping the comparison apples-to-apples.

export class SlidingWindowAccuracy {
  readonly #windowSize: number;
  readonly #window: boolean[] = [];

  constructor(windowSize: number) {
    if (!Number.isInteger(windowSize) || windowSize <= 0) {
      throw new RangeError(
        `windowSize must be a positive integer, got ${windowSize}`,
      );
    }
    this.#windowSize = windowSize;
  }

  /** Records one prediction's correctness. Only confident, made predictions should be recorded as `true`/`false` -- a caller wanting "no prediction" to count as a miss should record `false` explicitly, matching its own accuracy definition. */
  record(correct: boolean): void {
    this.#window.push(correct);
    if (this.#window.length > this.#windowSize) {
      this.#window.shift();
    }
  }

  /** The fraction of recorded predictions within the current window that were correct. `0` before any prediction has been recorded. */
  get accuracy(): number {
    if (this.#window.length === 0) {
      return 0;
    }
    return this.#window.filter(Boolean).length / this.#window.length;
  }

  get sampleCount(): number {
    return this.#window.length;
  }
}
