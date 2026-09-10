// The trigram baseline (Requirement 13.3): plain frequency counting over
// (2-character context -> next character) pairs, argmax prediction.
// Arithmetic on counts, not a dependency (ENG-5/ENG-6) -- this is the
// baseline VAL-4's milestone must beat, not a library.

export class TrigramModel {
  readonly #counts = new Map<string, Map<string, number>>();

  /** Trains on every (2-char context -> next char) pair in `corpus`, accumulating counts (can be called more than once, or streamed via `observe`). */
  train(corpus: string): void {
    for (let i = 2; i < corpus.length; i++) {
      this.observe(corpus.slice(i - 2, i), corpus[i]!);
    }
  }

  /** Records one (context, actual next character) pair -- the streaming-friendly primitive `train` is built from. */
  observe(context: string, next: string): void {
    const key = context.slice(-2);
    let nextCounts = this.#counts.get(key);
    if (!nextCounts) {
      nextCounts = new Map();
      this.#counts.set(key, nextCounts);
    }
    nextCounts.set(next, (nextCounts.get(next) ?? 0) + 1);
  }

  /** The most frequently observed character following `context`'s last two characters, or `undefined` if this context was never observed. Ties resolve to whichever character was inserted first for that context (`Map` iteration order), deterministically. */
  predict(context: string): string | undefined {
    const key = context.slice(-2);
    const nextCounts = this.#counts.get(key);
    if (!nextCounts || nextCounts.size === 0) {
      return undefined;
    }
    let best: string | undefined;
    let bestCount = -1;
    for (const [char, count] of nextCounts) {
      if (count > bestCount) {
        bestCount = count;
        best = char;
      }
    }
    return best;
  }
}
