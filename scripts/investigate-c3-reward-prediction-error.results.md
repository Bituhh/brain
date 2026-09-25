# PLAN.md C3 -- dopamine as a reward prediction error, measured on VAL-4

Generated 2026-09-20T16:04:59.878Z. 6 conditions x 10 seeds x 15000 characters. Reference is B5's winner (README §12 decision 13), unchanged.

**Read the two differences separately.** "vs reference" is the effect of giving dopamine a producer at all (switching LRN-8's modulated reinforce/punish path on). "vs raw reward" is the effect of that producer carrying a prediction error rather than a reward. Only the second is what C3 is about; the first is the confound HANDOFF fact 14 warned would otherwise be attributed to it.

The bar to quote alongside any number here: **16.56%**, the "always guess space" mode baseline (README §13.12 item 7). A configuration below it has undone the only real progress this network has made.

### Confirmation seeds (seeds 11, 12, 13, 14, 15)

| condition | mean | vs reference | vs raw reward | per seed |
| --- | --- | --- | --- | --- |
| B5's winner (no reward signal at all, the reference) | 19.05% | +0.00 | +0.52 | 18.90%, 18.10%, 21.45%, 19.30%, 17.50% |
| raw reward: rewardSignal on, no baseline (dopamine as C3 found it) | 18.53% | -0.52 | +0.00 | 17.90%, 19.75%, 19.40%, 18.45%, 17.15% |
| RPE, tauEvents 50 (~last few dozen characters) | 19.00% | -0.05 | +0.47 | 18.90%, 18.00%, 21.45%, 19.30%, 17.35% |
| RPE, tauEvents 200 (~recent performance) | 19.00% | -0.05 | +0.47 | 18.90%, 18.00%, 21.45%, 19.30%, 17.35% |
| RPE, tauEvents 1000 (~a fifteenth of the corpus) | 19.00% | -0.05 | +0.47 | 18.90%, 18.00%, 21.45%, 19.30%, 17.35% |
| baseline configured, nothing rewards (exactness control -- must equal the reference bit-identically) | 19.05% | +0.00 | +0.52 | 18.90%, 18.10%, 21.45%, 19.30%, 17.50% |

### Selection seeds (seeds 1, 2, 3, 4, 5)

| condition | mean | vs reference | vs raw reward | per seed |
| --- | --- | --- | --- | --- |
| B5's winner (no reward signal at all, the reference) | 20.36% | +0.00 | +0.54 | 19.85%, 20.50%, 21.10%, 21.15%, 19.20% |
| raw reward: rewardSignal on, no baseline (dopamine as C3 found it) | 19.82% | -0.54 | +0.00 | 19.20%, 21.30%, 21.20%, 18.90%, 18.50% |
| RPE, tauEvents 50 (~last few dozen characters) | 20.36% | +0.00 | +0.54 | 19.85%, 20.50%, 21.10%, 21.15%, 19.20% |
| RPE, tauEvents 200 (~recent performance) | 20.36% | +0.00 | +0.54 | 19.85%, 20.50%, 21.10%, 21.15%, 19.20% |
| RPE, tauEvents 1000 (~a fifteenth of the corpus) | 20.36% | +0.00 | +0.54 | 19.85%, 20.50%, 21.10%, 21.15%, 19.20% |
| baseline configured, nothing rewards (exactness control -- must equal the reference bit-identically) | 20.36% | +0.00 | +0.54 | 19.85%, 20.50%, 21.10%, 21.15%, 19.20% |

### Exactness control

**PASS** -- a configured-but-unfed baseline reproduces the reference exactly on all 10 seeds, so C3 left every pre-C3 configuration alone.

### Why the three time constants are indistinguishable

`tauEvents` does reach the native layer and does change the expectation's trajectory. Sampling `expectedReward()` every 1,000 characters on the real VAL-4 stream, with an identical reward sequence across taus:

| character | 0      | 1000   | 2000   | 3000   | 4000   | 5000   |
| --------- | ------ | ------ | ------ | ------ | ------ | ------ |
| tau=50    | 0.0198 | 0.2081 | 0.2081 | 0.2081 | 0.2081 | 0.2081 |
| tau=200   | 0.0050 | 0.2007 | 0.2020 | 0.2020 | 0.2020 | 0.2020 |
| tau=1000  | 0.0010 | 0.1270 | 0.1734 | 0.1905 | 0.1967 | 0.1991 |

They differ only in how _fast_ they reach the same place. VAL-4's reward stream is stationary -- the hit rate sits near 20% for the whole run -- so every time constant converges on that same expectation, and by character ~3,000 even the slowest is within 0.01 of the fastest. The reported figure is the accuracy over the **final** sliding window, by which point the three are indistinguishable. A task with a real contingency switch would separate them; this one cannot, for the same reason C2's surprise channel was inert here (HANDOFF fact 12).

**Not fully diagnosed, and recorded as such:** the three taus match not only on accuracy but on cumulative structural counts, to the synapse. The early trajectories genuinely differ, so something is quantising the difference away -- most plausibly that permanence deltas cross the `[0, 1]` clamp and the connection threshold after the same _integer_ number of events at every level in this range, making the resulting topology a step function of the gate rather than a continuous one. That is an inference from the evidence here, not a measurement, and it is worth settling before any later item tunes a modulator gain.

All trials completed.
