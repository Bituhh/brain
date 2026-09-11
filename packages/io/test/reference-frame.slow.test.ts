// Reference frames (NET-9, Phase 5.5 Requirement 6): the disambiguation
// task and its ablation, following this project's own established
// preference (`columns_and_voting.rs`'s module doc) for a deterministic,
// mechanism-level proof over a noisy statistical one where a clean one is
// available. The claim under test: a location column's activity can
// depolarise (NEU-6, via NET-5's existing `connect_lateral_voting`
// mechanism, wired once at construction) a *specific* sensory neuron only
// when the *specific* location bits that were wired near it are the ones
// active -- so the same weak sensory drive spikes when paired with one
// location and does not when paired with a different, non-overlapping one.
// This is the substrate capability Requirement 6 AC2 asks be demonstrated;
// it does not require a multi-episode learning curve to show the substrate
// can do it, any more than NET-5's own voting ablation needed one.

import { test } from "node:test";
import assert from "node:assert/strict";
import { Simulation, type LifConfig, type SimulationOptions, type ColumnConfig, type VotingGroupConfig } from "@brain/core";
import { wrapColumnHandles } from "../src/columns.ts";
import { encodeLocation, type LocationEncoderConfig } from "../src/location.ts";
import { makeSdr } from "../src/sdr.ts";

const VOTE_SEGMENT = 1;
const LOCATION_WIDTH = 20; // one module, one-hot: encodeLocation(x) sets exactly bit x
const WEAK_CURRENT = 0.6; // alone, never crosses threshold 1.0 (mirrors columns_and_voting.rs's own WEAK_CURRENT)
const THRESHOLD = 1.0;
const THRESHOLD_REDUCTION = 0.6; // matches columns_and_voting.rs: boosts effective threshold to 1.0 - 0.6 = 0.4 < WEAK_CURRENT's steady state
const TICKS = 60;

function locationConfig(): LocationEncoderConfig {
  return { modules: [{ period: LOCATION_WIDTH, width: LOCATION_WIDTH, activeBits: 1 }] };
}

/** A single one-hot x-coordinate SDR (y fixed at 0, which always lands on its own dedicated bit and never overlaps the x-block tested here). */
function locationSdrForX(x: number) {
  return encodeLocation(locationConfig(), { x, y: 0 });
}

function buildNetwork(sensoryBaseX: number, wireVoting: boolean) {
  // predictiveThresholdReduction (NEU-6): a fired vote_segment depolarises
  // the sensory neuron, lowering its effective threshold enough that
  // WEAK_CURRENT alone (otherwise insufficient) can cross it -- exactly
  // `columns_and_voting.rs`'s own mechanism, reused here for location
  // instead of another sensory column.
  const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0, tauPredictiveTicks: 1000, predictiveThresholdReduction: THRESHOLD_REDUCTION };
  const options: SimulationOptions = { maxDelay: 4, connectionThreshold: 0.5, synapseCapPerNeuron: 8 };
  const sim = Simulation.create(lif, options);

  const locationColumn: ColumnConfig = {
    neuronCount: LOCATION_WIDTH * 2, // x-block (20 bits) + y-block (20 bits), matching encodeLocation's per-module width*2
    threshold: THRESHOLD,
    excitatoryFraction: 1.0,
    baseX: 0,
    baseY: 0,
    baseZ: 0,
    internalPolicy: { p0: 0.0, lengthScale: 1.0, delayMin: 1, delayMax: 1, initialPermanence: 0.9 },
    neighbourhoodSize: LOCATION_WIDTH * 2,
    k: LOCATION_WIDTH * 2, // no within-column competition -- every driven bit may fire
    segments: { segmentsPerNeuron: 1, coincidenceThreshold: 1 },
  };
  const sensoryColumn: ColumnConfig = {
    neuronCount: 1, // a single "symbol X detector" neuron is enough for this mechanism-level proof
    threshold: THRESHOLD,
    excitatoryFraction: 1.0,
    baseX: sensoryBaseX,
    baseY: 0,
    baseZ: 0,
    internalPolicy: { p0: 0.0, lengthScale: 1.0, delayMin: 1, delayMax: 1, initialPermanence: 0.9 },
    neighbourhoodSize: 1,
    k: 1,
    segments: { segmentsPerNeuron: 2, coincidenceThreshold: 1 },
  };
  // Distance-only wiring: a location neuron and the sensory neuron connect
  // only when they are coordinate-close, under a short length_scale --
  // *except* at distance exactly 0, where probability_at is p0 regardless
  // of length_scale (exp(0) = 1 always). So the ablation (Requirement 6
  // AC4) is "never call connect_lateral_voting at all" -- matching
  // `columns_and_voting.rs`'s own established ablation shape exactly --
  // rather than trying to shrink length_scale to zero, which cannot
  // actually sever a distance-0 pair.
  const votingGroups: VotingGroupConfig[] = wireVoting
    ? [{ columnIds: [0, 1], voteSegment: VOTE_SEGMENT, policy: { p0: 1.0, lengthScale: 1.0, delayMin: 1, delayMax: 1, initialPermanence: 0.9 } }]
    : [];
  const handles = sim.buildColumns(1n, [locationColumn, sensoryColumn], votingGroups);
  const [location, sensory] = wrapColumnHandles(handles);
  return { sim, location: location!, sensory: sensory! };
}

/**
 * Drives `location` with the one-hot SDR for x-coordinate `atX` and
 * `sensory` with a weak, alone-insufficient symbol drive, for `TICKS`
 * ticks. Returns whether the sensory neuron ever spiked.
 */
function runTrial(sensoryBaseX: number, wireVoting: boolean, atX: number): boolean {
  const { sim, location, sensory } = buildNetwork(sensoryBaseX, wireVoting);
  const locationSdr = locationSdrForX(atX);
  let sensorySpiked = false;
  for (let tick = 0; tick < TICKS; tick++) {
    location.stimulateSdr(sim, locationSdr, 5.0); // strong: location column reliably represents "here"
    sensory.stimulateSdr(sim, makeSdr(1, [0]), WEAK_CURRENT);
    const spiked = sim.step();
    if (spiked.includes(sensory.range.start)) {
      sensorySpiked = true;
    }
  }
  return sensorySpiked;
}

test("a sensory neuron depolarised by a specific location spikes under otherwise-insufficient drive when that location is active (Requirement 6 AC2)", () => {
  // sensoryBaseX chosen so the sensory neuron's coordinate coincides with
  // location x=5's neuron coordinate exactly (distance 0) and is far
  // (length_scale-relative) from every other location neuron, including
  // x=15's.
  const sensoryBaseX = 5;
  assert.equal(runTrial(sensoryBaseX, true, 5), true, "location x=5 (coordinate-adjacent to the sensory neuron) must depolarise it enough for the weak symbol drive to cross threshold");
});

test("the same weak drive does NOT spike under a different, non-overlapping location (Requirement 6 AC2's disambiguation)", () => {
  const sensoryBaseX = 5;
  assert.equal(runTrial(sensoryBaseX, true, 15), false, "location x=15 (coordinate-distant from the sensory neuron, under a short length_scale) must not depolarise it -- the same symbol at a different location produces a different response");
});

test("ablation: with lateral voting never wired at all, neither location depolarises the sensory neuron (Requirement 6 AC4)", () => {
  const sensoryBaseX = 5;
  assert.equal(runTrial(sensoryBaseX, false, 5), false, "without any voting connection, even the previously-depolarising location must fail to raise the weak drive above threshold");
});
