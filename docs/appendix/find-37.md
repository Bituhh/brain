# Finding 37 — raw data: periodic sweeps under partitioning (PLAN.md C11)

[2026-09-30 20:26 +0100]. Every row below is a run of `crates/brain-core/tests/partitioning_reference.rs` or `packages/brain/test/boundary.test.ts`. The scenario is `build_network_with_prune_canary(7)`: two 10-neuron columns, one per partition, 200 ticks. The comparison is against a plain single-threaded `Scheduler`. A "mutation" is a one-line change made temporarily to show that a fix is load-bearing, reverted straight after.

## 1. What the existing tests were hiding (before any C11 fix)

The spatial-sweep test (`a_spatial_sprout_sweep_is_identical_across_partitioning_and_threading`) and the always-on test compared neuron state and the **count** of occupied synapses. A probe comparing every synapse field by field, at 2 partitions (sequential):

| Condition | Synapses differing (weight, eligibility or delay) | Delay differs | Ticks whose spike set differs |
| --- | --- | --- | --- |
| Spatial sweep, `min_cross_partition_delay: 2` (as shipped) | 102 | 102 | 0 |
| Spatial sweep, delay set to 1 | 30 | 0 | 0 |
| Spatial sweep, delay 1, boundary set refreshed after a sprouting sweep | 0 | 0 | 0 |

Synapse **ids** matched in every run, so the test's stated reason for comparing counts ("sprouting may use a different free slot") did not hold.

The always-on test (homeostatic scaling plus structural plasticity, which only prunes here), once it compared synapses field by field, failed on synapse 2's weight: **0.5** single-threaded, **1.0** partitioned. A per-tick trace with scaling alone isolated the cause. At tick 30, neuron 13 spiked, and synapse 122 (3 → 13) is a cross-partition dendritic synapse. Single-threaded applied `on_post_spike` in that tick (eligibility 0.0123, weight 1.0). Partitioned deferred it to tick 31. That is harmless on its own, because the next tick's deliveries read it after it lands, but a scaling sweep that runs on the tick of the spike rescales the pre-update weight. After moving the application to the end of the same tick, weights and eligibility matched **at every tick**, not only at the end.

## 2. The C11 scenario (`every_periodic_sweep_is_bit_identical_across_partitioning_and_threading`)

Six configurations (each sweep alone, then all five together), each against 1 partition, 2 partitions sequential, 2 partitions on rayon with 4 threads, and 2 partitions on the pinned executor with 4 threads. The test compares every tick's spike and veto sets, every partition's inhibition `k` every tick, every neuron field, every synapse field (including target, segment and delay), and every composite's segment-threshold state read from its owner. **All 24 comparisons pass.**

The canary (`the_c11_scenario_exercises_every_sweep`): with all five on, switching off any one changes the run (spikes, weights, neuron thresholds, segment thresholds or `k`). `k` moves off 2. Segment thresholds drift in both columns. The structural sweep adds synapses across the partition boundary. **All pass.**

## 3. Mutation checks: each fix is load-bearing

| Mutation (reverted after) | First failure in the C11 test |
| --- | --- |
| No segment-state length sync | The length assert (19 composites against a shorter partition). With the assert disabled and each composite read from its owner: composite 9 (column A's last) ends at **1.1458** partitioned against **0.9460** single-threaded, on the segment-threshold configuration at 2 partitions |
| No boundary refresh after a sprouting sweep | Structural plasticity, 2 partitions: synapse 172's weight |
| Inhibition homeostasis observes partition 0's spikes only, not the merged count | Inhibition homeostasis, 2 partitions: `k` differs at **tick 30** |
| `on_post_spike` messages applied at the start of the next tick (the pre-C11 order) | Measured on the always-on test (section 1), before the C11 test existed |
| Boundary table published before the sweeps (the pre-C11 order) | **Not detectable.** No plasticity rule reads `NeuronLocal::rate_estimate`, the one field a sweep writes. The order is fixed because it is the correct one, not claimed as tested |

## 4. Through the FFI (`boundary.test.ts`, C11 block)

16 neurons, even split, inhibition neighbourhoods of 4 (aligned with both the 2- and the 4-partition split), all five sweeps configurable, STDP on, three competing stimuli per neighbourhood per tick.

| Test | Result |
| --- | --- |
| Each sweep alone and all five: `threadCount` 2 and 4 reproduce `threadCount` 1 exactly (spikes every tick, thresholds, every synapse's weight, permanence, delay, target and occupancy, `segmentThresholdStats`, `structuralStats`) | 12 of 12 pass |
| At `threadCount` 2, switching off any one sweep changes the run, and sprouts cross the partition boundary | Pass |
| The same test against the pre-C11 wiring (whole-network sweeps not attached to the runtime, segment sweep not run partitioned) | **Fails** on the first sweep checked: "switching off scaling at threadCount 2 must change the run" |
| `minCrossPartitionDelay: 2` refused at `threadCount` 2, accepted at 1 | Pass |
| `inhibitionHomeostasis` without `inhibition` refused at `threadCount` 1 and 2 | Pass |

A calibration note, recorded because the first version of the FFI scenario was vacuous for one sweep. With one stimulus per tick, every inhibition neighbourhood had at most one candidate, so `k` never mattered and switching inhibition homeostasis off changed nothing. A probe measured 67 spikes in 200 ticks with and without it, then 200 and 200 at a lower threshold. With three competing stimuli per neighbourhood it measured 599 spikes without it and 505 with it.
