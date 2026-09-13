// The streaming experiment harness (IO-4, Requirement 9): feeds a source
// of sequential inputs to the network with no train/inference split.
// A generator, not a class with a start/stop/mode flag -- there is no
// "training mode" to turn off (Requirement 9.2); a caller iterates
// indefinitely, or stops at any point and calls `sim.snapshot()`
// (Requirement 9.4, unchanged from Phase 0-4) to resume the stream later
// without discarding what was learned.

// Driveable from plain TypeScript using only @brain/io and @brain/core, no
// new runtime dependency (Requirement 9.5, ENG-6): everything below is
// pure orchestration over this workspace's own modules and Node built-ins.
import type { Simulation } from "@brain/core";
import type { Sdr } from "../sdr.ts";
import { decode, type Candidate, type DecodeResult } from "../decoders/overlap.ts";
import type { ColumnHandle } from "../columns.ts";

export interface StreamStep<T, L> {
  readonly input: T;
  readonly predicted: DecodeResult<L> | undefined;
  readonly actual: L;
  /**
   * The primary column's observed activity this step was decoded from, or
   * `undefined` when no primary column exists. Exposed so a caller can run
   * its own analysis beyond `decode`'s single winning candidate -- e.g.
   * `rankByOverlapFraction` (`decoders/overlap.ts`) for a representational-
   * collision signal (NET-10) -- without this harness needing to know
   * about that caller's own purpose. Already computed internally to
   * produce `predicted`; this just stops discarding it.
   */
  readonly observed: Sdr | undefined;
}

export interface StreamThroughOptions<T, L> {
  readonly source: Iterable<T>;
  readonly encode: (input: T) => Sdr;
  /**
   * Every column in this group receives the identical encoded `Sdr` each
   * step (a voting group all seeing the same input); the *first* column's
   * resulting activity is what gets decoded, matching NET-5's "read off
   * any one column once its neighbours have voted" pattern -- lateral
   * voting (if the caller wired it via `buildColumns`' `votingGroups`) is
   * what makes that column's answer representative of the whole group's
   * consensus, not a special case this harness needs to know about.
   */
  readonly columns: ReadonlyArray<ColumnHandle>;
  readonly sim: Simulation;
  readonly candidates: ReadonlyArray<Candidate<L>>;
  readonly actualLabelOf: (input: T) => L;
  readonly ticksPerInput: number;
  /** Current used to stimulate each active bit. Defaults to 10.0, comfortably supra-threshold for this project's own test/example networks. */
  readonly stimulateCurrent?: number;
  /** Passed straight to `decode` (Requirement 7.2's confidence threshold). Defaults to 0.3. */
  readonly minConfidence?: number;
}

/**
 * Streams `options.source` through the network with learning continuously
 * on (Requirement 9.1): each `next()` call encodes one input, stimulates
 * it onto every configured column, advances `ticksPerInput` ticks,
 * decodes the resulting activity, and yields. Plasticity (STDP,
 * three-factor, homeostasis, structural plasticity/growth) stays exactly
 * as active as `sim`'s own configuration made it -- this function
 * introduces no new plasticity code path and no mode switch of any kind
 * (Requirement 9.2).
 */
export function* streamThrough<T, L>(options: StreamThroughOptions<T, L>): Generator<StreamStep<T, L>> {
  const { source, encode, columns, sim, candidates, actualLabelOf, ticksPerInput } = options;
  const current = options.stimulateCurrent ?? 10.0;
  const minConfidence = options.minConfidence ?? 0.3;
  const [primary] = columns;

  for (const input of source) {
    const sdr = encode(input);
    for (const column of columns) {
      column.stimulateSdr(sim, sdr, current);
    }
    let spiked: number[] = [];
    for (let tick = 0; tick < ticksPerInput; tick++) {
      spiked = sim.step();
    }
    const observed = primary ? primary.observedSdr(spiked) : undefined;
    const predicted = observed ? decode(observed, candidates, minConfidence) : undefined;
    yield { input, predicted, actual: actualLabelOf(input), observed };
  }
}
