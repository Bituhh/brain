// The sensorimotor loop (IO-5, Requirement 16): entirely TypeScript, over
// the FFI that already exists. No Rust changes at all -- `step()` already
// returns the indices that spiked and `stimulate()` already takes input
// back in, so this is a `while` statement in orchestration code, not a
// core capability (Requirement 16.1).
//
// Deliberately its own function, not `harness/stream.ts`'s `streamThrough`
// called with an environment-backed source, and not a wrapper around it
// either: `streamThrough` decodes a *prediction* and compares it to a
// known ground-truth label drawn from a predetermined source, which can
// run ahead of the network's own responses. This loop cannot -- the next
// observation is `environment.act(decoded)`'s *consequence*, so nothing
// before this tick's decode exists to source it from, and there is no
// ground-truth label to compare against (only whether the closed loop
// measurably outperforms a disconnected one on a task that requires
// acting, Requirement 16.5's ablation). Both functions are a few lines of
// orchestration once `ColumnHandle`/`decode()` exist, so sharing that much
// is enough.

import type { Simulation } from "@brain/core";
import type { Sdr } from "./sdr.ts";
import { decode, type Candidate } from "./decoders/overlap.ts";
import type { ColumnHandle } from "./columns.ts";

/** The environment half of the loop (Requirement 16.1) -- `GridWorld` (`environments/grid.ts`) is this phase's only implementation. */
export interface Environment<Obs, Act> {
  observe(): Obs;
  act(action: Act): void;
}

export interface SensorimotorConfig<Obs, Act> {
  readonly encode: (observation: Obs) => Sdr;
  /** Candidate SDRs, one per possible action -- decoded against via Requirement 7's existing SDR-overlap readout, not a second, motor-specific path (Requirement 16.2). */
  readonly actions: ReadonlyMap<Act, Sdr>;
  /** Every column in this group receives the identical encoded observation each step; the *first* column's activity is decoded (matching `streamThrough`'s own convention). */
  readonly columns: ReadonlyArray<ColumnHandle>;
  readonly sim: Simulation;
  readonly ticksPerStep: number;
  readonly stimulateCurrent?: number;
  readonly minConfidence?: number;
}

export interface SensorimotorStep<Obs, Act> {
  readonly observation: Obs;
  /** `undefined` when the network did not commit to an action this step (no candidate cleared `minConfidence`) -- the environment then receives no `act` call this step, which is a legitimate outcome for a spiking network, not an exception. */
  readonly action: Act | undefined;
}

/**
 * Runs the loop indefinitely: observe -> encode -> stimulate -> advance
 * `ticksPerStep` -> decode -> (maybe) act -> yield. Unlike `streamThrough`,
 * there is no source to exhaust -- environment interaction is continuous
 * by nature (IO-5), so this generator never completes on its own; a
 * caller stops by breaking out of its `for...of` loop or simply not
 * calling `.next()` again.
 */
export function* runSensorimotorLoop<Obs, Act>(environment: Environment<Obs, Act>, config: SensorimotorConfig<Obs, Act>): Generator<SensorimotorStep<Obs, Act>> {
  const current = config.stimulateCurrent ?? 10.0;
  const minConfidence = config.minConfidence ?? 0.3;
  const [primary] = config.columns;
  const candidates: Candidate<Act>[] = Array.from(config.actions.entries(), ([label, sdr]) => ({ label, sdr }));

  for (;;) {
    const observation = environment.observe();
    const sdr = config.encode(observation);
    for (const column of config.columns) {
      column.stimulateSdr(config.sim, sdr, current);
    }
    let spiked: number[] = [];
    for (let tick = 0; tick < config.ticksPerStep; tick++) {
      spiked = config.sim.step();
    }
    const decoded = primary ? decode(primary.observedSdr(spiked), candidates, minConfidence) : undefined;
    const action = decoded?.label;
    if (action !== undefined) {
      environment.act(action);
    }
    yield { observation, action };
  }
}
