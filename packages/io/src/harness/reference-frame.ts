// Reference frames (NET-9, Phase 5.5 Requirement 6): orchestration over
// the *existing* FFI and the *existing* NET-5 lateral-voting mechanism --
// no new core capability, matching how IO-5's sensorimotor loop was built
// in Phase 5. A location column and a sensory column are stimulated
// independently each step (location.ts's `encodeLocation`, and whatever
// sensory encoder the caller supplies); the binding between "this feature"
// and "this location" is `connect_lateral_voting`'s ordinary dendritic
// depolarisation (NEU-6), wired once at construction via `buildColumns`'
// `votingGroups`, exactly as NET-5 already validated for columns voting on
// a shared answer -- here the "vote" is a location column depolarising the
// sensory column's matching neurons, not two sensory columns agreeing.
//
// Deliberately its own function, not `harness/stream.ts`'s `streamThrough`
// called with a location-aware source: `streamThrough` compares a decoded
// prediction against a predetermined ground-truth label; this loop drives
// two columns per step from one environment and has no prediction to
// score, mirroring `loop.ts`'s own reasoning for being a separate function
// from `streamThrough` rather than a wrapper around it.

import type { Simulation } from "@brain/core";
import type { Sdr } from "../sdr.ts";
import type { ColumnHandle } from "../columns.ts";
import { PathIntegrator, encodeLocation, type LocationEncoderConfig, type Position } from "../location.ts";

/** The environment half of the loop -- structurally identical to `loop.ts`'s `Environment<Obs, Act>`. */
export interface Environment<Obs, Act> {
  observe(): Obs;
  act(action: Act): void;
}

export interface ReferenceFrameConfig<Obs, Act> {
  readonly encodeObservation: (observation: Obs) => Sdr;
  readonly locationConfig: LocationEncoderConfig;
  /** Path integration's own per-action displacement -- independent of the environment's own cursor state (`location.ts`'s `PathIntegrator`). */
  readonly actionDelta: (action: Act) => Position;
  /**
   * Chooses the next action. Deliberately caller-supplied rather than
   * decoded from network activity: action *selection* is NET-13's job
   * (Phase 5.5 Requirements 3-5), not this requirement's -- a
   * reference-frame experiment needs the agent to move, not to decide
   * intelligently how.
   */
  readonly chooseAction: () => Act;
  readonly locationColumn: ColumnHandle;
  readonly sensoryColumn: ColumnHandle;
  readonly sim: Simulation;
  readonly ticksPerStep: number;
  readonly stimulateCurrent?: number;
}

export interface ReferenceFrameStep<Obs> {
  readonly observation: Obs;
  readonly position: Position;
  /** The sensory column's own locally-indexed spiked bits this step (`ColumnHandle.observedSdr`'s convention). */
  readonly sensorySpiked: ReadonlyArray<number>;
}

/**
 * Runs indefinitely (mirroring `loop.ts`'s `runSensorimotorLoop`): each
 * step observes, encodes both the location and the sensory observation,
 * stimulates both columns, advances `ticksPerStep`, and yields. Learning
 * stays on throughout, exactly as `sim`'s own configuration already makes
 * it (no mode switch, same discipline as every other harness in this
 * package).
 */
export function* runReferenceFrameLoop<Obs, Act>(
  environment: Environment<Obs, Act>,
  config: ReferenceFrameConfig<Obs, Act>,
): Generator<ReferenceFrameStep<Obs>> {
  const current = config.stimulateCurrent ?? 10.0;
  const integrator = new PathIntegrator<Act>(config.actionDelta);

  for (;;) {
    const observation = environment.observe();
    const locationSdr = encodeLocation(config.locationConfig, integrator.position);
    const sensorySdr = config.encodeObservation(observation);
    config.locationColumn.stimulateSdr(config.sim, locationSdr, current);
    config.sensoryColumn.stimulateSdr(config.sim, sensorySdr, current);

    let spiked: number[] = [];
    for (let tick = 0; tick < config.ticksPerStep; tick++) {
      spiked = config.sim.step();
    }
    const sensorySpiked = config.sensoryColumn.observedSdr(spiked).activeBits;

    const action = config.chooseAction();
    integrator.integrate(action);
    environment.act(action);

    yield { observation, position: integrator.position, sensorySpiked };
  }
}
