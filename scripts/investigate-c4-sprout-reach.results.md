# PLAN.md C4 -- spatial sprout reach on VAL-4

Generated 2026-09-21T09:34:27.648Z by scripts/investigate-c4-sprout-reach.ts. Corpus slice 15000 characters; confirmation seeds 11, 12, 13, 14, 15, never used by the B5 value search to choose.

Winner these rows build on (tune-b5-values.chosen.json): C[fixes -2--] ref=1 thr=3 target=permanence homeo=1 lr=0.02 tau=4 dep=2 elig=50 unsil=0.3 gap=4 elim=20000.

**The bar to read every number against** is README §13.12 item 7's mode baseline, "always guess space": **16.56%**. A change that improves a delta but drops back under it has undone the only real progress this network has made. Trigram on this corpus is 29.07%; the VAL-4 milestone is not met either way.

Rows C, B and E are read from `investigate-b5-growth.checkpoint.jsonl` rather than re-run -- same key scheme, same seeds, same corpus slice.

`NG-*` rows carry the **overlapping-vs-disjoint control**: a radius gives every neuron its own candidate set where a block is shared, so candidate-pair counts change with no growth at all. Without these rows, movement in an `SB-*`/`SE-*` row would be unattributable between growth's capacity and the reach change itself.

| condition | mean | per seed | vs C | sprouted / unsilenced / eliminated / silent now (mean) | mean s/trial |
| --- | --- | --- | --- | --- | --- |
| C: no growth, index-block reach (B5 winner) | 19.05% | 18.90%, 18.10%, 21.45%, 19.30%, 17.50% | +0.00 | 56582 / 1242 / 0 / 55340 | 127 |
| B: C + growth, burst pace, index-block reach | 19.05% | 18.90%, 18.10%, 21.45%, 19.30%, 17.50% | +0.00 | 81259 / 1528 / 0 / 79731 | 72 |
| E: C + growth, gentle pace, index-block reach | 19.05% | 18.90%, 18.10%, 21.45%, 19.30%, 17.50% | +0.00 | 56732 / 1244 / 0 / 55488 | 64 |
| NG-25: no growth, spatial reach r=25 | 19.50% | 18.95%, 20.65%, 19.95%, 19.25%, 18.70% | +0.45 | 27882 / 529 / 0 / 27353 | 60 |
| NG-50: no growth, spatial reach r=50 | 19.86% | 19.15%, 20.35%, 21.20%, 19.70%, 18.90% | +0.81 | 55131 / 1145 / 0 / 53985 | 71 |
| NG-100: no growth, spatial reach r=100 | 19.62% | 20.75%, 19.55%, 20.10%, 18.70%, 19.00% | +0.57 | 118386 / 2432 / 0 / 103793 | 99 |
| SB-25: C + growth, burst pace, spatial reach r=25 | 19.34% | 20.20%, 20.15%, 19.30%, 18.10%, 18.95% | +0.29 | 62759 / 955 / 0 / 61804 | 66 |
| SB-50: C + growth, burst pace, spatial reach r=50 | 19.59% | 18.05%, 20.20%, 20.70%, 19.55%, 19.45% | +0.54 | 102696 / 1839 / 0 / 100857 | 78 |
| SB-100: C + growth, burst pace, spatial reach r=100 | 18.78% | 18.80%, 19.75%, 17.55%, 18.55%, 19.25% | -0.27 | 196635 / 4948 / 0 / 185630 | 110 |
| SE-50: C + growth, gentle pace, spatial reach r=50 | 19.85% | 19.15%, 20.35%, 21.20%, 19.70%, 18.85% | +0.80 | 56615 / 1178 / 0 / 55437 | 61 |

Per-seed identity is worth checking by eye as well as by mean: README §13.12 item 10's finding was that growth rows reproduced condition C's accuracy _identically on every seed_, which is a much stronger statement than their means agreeing.

The direct topology measurement (grown -> original synapse counts on the real network) is in `investigate-c4-sprout-reach.samples.md`, produced by re-running this script with `C4_INSTRUMENT=1`.
