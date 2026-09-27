# Appendix: finding 26 — the one-variable sprouting ablation at two horizons

Raw data for [`findings.md` finding 26](../findings.md). Measured 2026-09-27 [2026-09-27 10:30 +0100] by `scripts/investigate-corpus-horizon.ts`, protocol `corpus-horizon-v2-postfix`, condition **`D-no-sprout`**, seeds 1–3, 6 trials (3 seeds × {200,000, 15,000} characters), on the post-fix engine (decision 27). Run time: 15,000-character trials ~85 s each, 200,000-character trials ~1,505 s each, three concurrent.

## What the condition is, and why it had to be added

`D-no-sprout` is **B5's winner with `structuralPlasticity` removed and nothing else changed** — VAL-9's shape, one variable. It is built by destructuring the winner's config rather than by setting the field to `undefined`, so the checkpoint key carries no stray entry. `silentSynapses` is deliberately **left on** even though nothing sprouts, because the point is to vary one field, not to tidy up after it.

It exists because the only long-horizon comparison available before it was `A-b5` against `C-default`, and `C-default` is `DEFAULT_CONFIG`, which differs from B5's winner in **at least seven mechanisms simultaneously**: no `plasticity` at all (so STDP never runs), no structural plasticity, no homeostatic scaling, no silent synapses, no tonic acetylcholine, Count-mode rather than weighted dendritic votes, and a different coincidence threshold. Any claim of the form "structural plasticity does X to the trajectory" drawn from that comparison is confounded seven ways. This one is not.

## Result 1 — sprouting's benefit is real, and it is what clears the bar at 15,000

| @15,000 characters | seed 1 | seed 2 | seed 3 | mean | vs 16.56% bar |
| --- | --- | --- | --- | --- | --- |
| `A-b5` (sprouting on) | 19.85% | 20.50% | 21.10% | **20.48%** | **+3.92** |
| `D-no-sprout` | 14.15% | 17.15% | 17.80% | **16.37%** | **−0.19** |
| **sprouting is worth** | +5.70 | +3.35 | +3.30 | **+4.12** |  |

**This is the first one-variable confirmation of B5's central claim.** B5 reported a sprout-disabled control at 15.58% and concluded sprouting was what lifted VAL-4 over the "always guess space" bar; that control varied more than one field. With exactly one field varied, removing sprouting lands at **16.37%, which is 0.19 points _below_ the 16.56% bar** — so B5's claim survives a proper ablation: structural plasticity is the mechanism responsible for the only real ground VAL-4 has gained.

## Result 2 — sprouting does make the peak arrive earlier

| condition     | seed | peak accuracy | peak at    |
| ------------- | ---- | ------------- | ---------- |
| `A-b5`        | 1    | 23.75%        | **17,250** |
| `A-b5`        | 2    | 24.15%        | **17,250** |
| `A-b5`        | 3    | 24.65%        | **17,250** |
| `D-no-sprout` | 1    | 22.30%        | **55,250** |
| `D-no-sprout` | 2    | 22.60%        | **52,000** |
| `D-no-sprout` | 3    | 24.50%        | 17,250     |

On two of three seeds the peak moves from ~52,000–55,000 characters to 17,250 — **roughly three times sooner** — and peak height is slightly higher with sprouting (23.75–24.65% against 22.30–24.50%). Seed 3 peaks at 17,250 either way.

**A caveat on the peak location that applies to both conditions.** `A-b5` peaks at _exactly_ 17,250 characters on all three seeds, and so does `D-no-sprout`'s seed 3. A learning milestone would not land on the same character across independent seeds; a locally easy passage in the corpus would. So "peaks at 17,250" is partly a property of the corpus, and the defensible reading of Result 2 is the _block-level_ one — sprouting reaches its plateau in the low tens of thousands of characters where the ablation takes until the fifties.

## Result 3 — the headline: sprouting does NOT cause the decline

| condition     | seed | peak   | @200,000      | drop         |
| ------------- | ---- | ------ | ------------- | ------------ |
| `A-b5`        | 1    | 23.75% | 17.2%         | **6.5 pts**  |
| `A-b5`        | 2    | 24.15% | 14.9%         | **9.2 pts**  |
| `A-b5`        | 3    | 24.65% | 14.3%         | **10.3 pts** |
|               |      |        | **mean drop** | **8.67 pts** |
| `D-no-sprout` | 1    | 22.30% | 13.9%         | **8.4 pts**  |
| `D-no-sprout` | 2    | 22.60% | 14.3%         | **8.3 pts**  |
| `D-no-sprout` | 3    | 24.50% | 14.8%         | **9.7 pts**  |
|               |      |        | **mean drop** | **8.80 pts** |

**The decline is the same size with sprouting and without it — 8.67 against 8.80 points, a 0.13-point difference on three seeds, which is nothing.** Structural plasticity is therefore not the cause of the post-peak decline, and disabling it does not protect against it.

Full trajectories:

| chars   | A s1  | A s2  | A s3  | D s1  | D s2  | D s3  |
| ------- | ----- | ----- | ----- | ----- | ----- | ----- |
| 15,000  | 19.9% | 20.5% | 21.1% | 14.1% | 17.2% | 17.8% |
| 35,000  | 20.1% | 19.7% | 21.6% | 19.9% | 18.7% | 20.4% |
| 50,000  | 18.8% | 18.6% | 20.8% | 17.9% | 20.0% | 20.6% |
| 100,000 | 18.4% | 17.4% | 17.0% | 17.8% | 19.8% | 19.1% |
| 200,000 | 17.2% | 14.9% | 14.3% | 13.9% | 14.3% | 14.8% |

| @200,000 characters | seed 1 | seed 2 | seed 3 | mean | vs 16.25% bar |
| --- | --- | --- | --- | --- | --- |
| `A-b5` | 17.00% | 14.90% | 14.05% | **15.32%** | **−0.93** |
| `D-no-sprout` | 13.75% | 14.30% | 14.80% | **14.28%** | **−1.97** |
| **sprouting is worth** | +3.25 | +0.60 | −0.75 | **+1.03** |  |

So sprouting's advantage shrinks from **+4.12 points at 15,000 to +1.03 at 200,000** — but _not_ because sprouting degrades. It shrinks because a decline the two conditions share dominates the comparison, and because the ablation is still climbing at 15,000 where the sprouting condition has already peaked. On seed 3 the sign even reverses (−0.75), so at 200,000 the +1.03 mean is not a result worth defending on three seeds.

## Result 4 — permanence saturation is present in BOTH, and worse without sprouting

End-of-run permanence distribution at 200,000 characters (sampled once per trial, not a trajectory):

| condition | seed | `atOne`/`occupied` | `connected`/`occupied` | `occupied` | `sumWeight` (target 4,800) |
| --- | --- | --- | --- | --- | --- |
| `A-b5` | 1 | 56.0% | 69.7% | 96,556 | 4,848 |
| `A-b5` | 2 | 49.9% | 69.1% | 95,963 | 4,850 |
| `A-b5` | 3 | 59.3% | 71.7% | 96,499 | 4,851 |
| `D-no-sprout` | 1 | **66.2%** | 78.4% | 31,893 | 4,808 |
| `D-no-sprout` | 2 | **69.7%** | 80.5% | 32,091 | 4,810 |
| `D-no-sprout` | 3 | **66.8%** | 77.3% | 31,852 | 4,809 |

**Two-thirds of the ablation's synapses end pinned at maximum permanence, against half to three-fifths of the sprouting condition's** — so saturation is not caused by sprouting either, and is in fact _worse_ without it. That is consistent with sprouting diluting saturation by continually adding fresh low-permanence contacts, which is a mechanism worth testing rather than a conclusion.

Note `occupied` holds at ~31,900 for the ablation, exactly matching `C-default`'s ~31,893 — with no structural plasticity, the synapse count stays at the initial wiring, as it must. `sumWeight` is on target in both conditions, confirming decision 27's homeostatic fix independently of sprouting.

## What this settles and what it does not

**Settles:** structural plasticity helps VAL-4 reach its peak sooner and slightly higher, and is the mechanism that clears the "always guess space" bar at the protocol's horizon. It does **not** cause the post-peak decline, and removing it does not prevent the decline.

**Does not settle:** what _does_ cause the decline. The leading candidate is unregulated permanence — `HomeostaticScaling` acts on `weight`, not `permanence` (decision 11's split), and nothing in the engine regulates permanence's distribution, while dendritic coincidence detection is gated on it. Saturation is present in both conditions at 66–70% and 50–59%. But these are endpoints, not trajectories, so nothing here shows whether saturation leads the decline or trails it. That is the next measurement, and it is a small harness change: sample the permanence-distribution fields on the existing 5,000-character `sparse` cadence rather than once at the end.
