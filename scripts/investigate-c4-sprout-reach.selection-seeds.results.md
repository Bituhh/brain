# PLAN.md C4 -- spatial sprout reach on VAL-4

Generated 2026-09-21T12:53:44.471Z by scripts/investigate-c4-sprout-reach.ts. Corpus slice 15000 characters; confirmation seeds 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, never used by the B5 value search to choose.

Winner these rows build on (tune-b5-values.chosen.json): C[fixes -2--] ref=1 thr=3 target=permanence homeo=1 lr=0.02 tau=4 dep=2 elig=50 unsil=0.3 gap=4 elim=20000.

**The bar to read every number against** is README §13.12 item 7's mode baseline, "always guess space": **16.56%**. A change that improves a delta but drops back under it has undone the only real progress this network has made. Trigram on this corpus is 29.07%; the VAL-4 milestone is not met either way.

Rows C, B and E are read from `investigate-b5-growth.checkpoint.jsonl` rather than re-run -- same key scheme, same seeds, same corpus slice.

`NG-*` rows carry the **overlapping-vs-disjoint control**: a radius gives every neuron its own candidate set where a block is shared, so candidate-pair counts change with no growth at all. Without these rows, movement in an `SB-*`/`SE-*` row would be unattributable between growth's capacity and the reach change itself.

| condition | mean | per seed | vs C | sprouted / unsilenced / eliminated / silent now (mean) | mean s/trial |
| --- | --- | --- | --- | --- | --- |
| C: no growth, index-block reach (B5 winner) | 19.81% | 19.85%, 20.50%, 21.10%, 21.15%, 19.20%, 19.60%, 18.90%, 19.90%, 19.50%, 18.45% | +0.00 | 56496 / 1211 / 0 / 55285 | 124 |
| B: C + growth, burst pace, index-block reach | 19.81% | 19.85%, 20.50%, 21.10%, 21.15%, 19.20%, 19.60%, 18.90%, 19.90%, 19.50%, 18.45% | +0.00 | 78264 / 1416 / 0 / 76840 | 76 |
| E: C + growth, gentle pace, index-block reach | 19.81% | 19.85%, 20.50%, 21.10%, 21.15%, 19.20%, 19.60%, 18.90%, 19.90%, 19.50%, 18.45% | +0.00 | 56533 / 1211 / 0 / 55322 | 68 |
| NG-25: no growth, spatial reach r=25 | 18.98% | 17.70%, 19.10%, 18.40%, 18.20%, 19.70%, 18.30%, 19.50%, 20.25%, 20.10%, 18.50% | -0.84 | 27904 / 528 / 0 / 27376 | 57 |
| NG-50: no growth, spatial reach r=50 | 19.97% | 20.30%, 20.65%, 20.15%, 18.75%, 19.80%, 20.25%, 18.45%, 21.35%, 19.90%, 20.15% | +0.16 | 55014 / 1107 / 0 / 53906 | 67 |
| NG-100: no growth, spatial reach r=100 | 19.30% | 19.80%, 20.40%, 20.10%, 19.05%, 17.15%, 19.95%, 20.35%, 18.30%, 19.60%, 18.30% | -0.51 | 117911 / 2386 / 0 / 103819 | 96 |
| SB-25: C + growth, burst pace, spatial reach r=25 | 19.25% | 17.45%, 19.60%, 18.85%, 20.40%, 19.40%, 18.50%, 20.15%, 20.70%, 18.50%, 19.00% | -0.56 | 61951 / 856 / 0 / 61095 | 67 |
| SB-50: C + growth, burst pace, spatial reach r=50 | 19.19% | 19.15%, 19.30%, 19.00%, 19.05%, 18.20%, 18.80%, 18.90%, 20.50%, 20.20%, 18.80% | -0.62 | 103084 / 1662 / 0 / 101389 | 81 |
| SB-100: C + growth, burst pace, spatial reach r=100 | 19.25% | 19.40%, 20.10%, 21.30%, 19.85%, 19.00%, 18.90%, 18.60%, 17.60%, 19.00%, 18.75% | -0.57 | 182438 / 5577 / 0 / 171164 | 122 |
| SE-50: C + growth, gentle pace, spatial reach r=50 | 19.89% | 20.30%, 20.65%, 20.15%, 18.65%, 19.05%, 20.25%, 18.45%, 21.35%, 19.90%, 20.15% | +0.08 | 55784 / 1116 / 0 / 54658 | 66 |

Per-seed identity is worth checking by eye as well as by mean: README §13.12 item 10's finding was that growth rows reproduced condition C's accuracy _identically on every seed_, which is a much stronger statement than their means agreeing.

The direct topology measurement (grown -> original synapse counts on the real network) is in `investigate-c4-sprout-reach.samples.md`, produced by re-running this script with `C4_INSTRUMENT=1`.
