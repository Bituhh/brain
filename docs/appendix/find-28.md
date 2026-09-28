# Appendix: finding 28 — contributor-gated reinforcement and soft-bound permanence, measured

Raw data for [`findings.md` finding 28](../findings.md). Measured 2026-09-28 [2026-09-28 09:44 +0100] by `scripts/investigate-c14-credit-and-bounds.ts`, protocol `c14-credit-and-bounds-v1`. **112 trials**: ten conditions × the official ten seeds at 15,000 characters, plus four conditions × three seeds at 200,000. 12 concurrent workers, 61 minutes wall clock. PLAN.md C14.

**Every reading was written into the script's header before any trial ran**, with one amendment made after the smoke path and before the real run, recorded in place and repeated below.

## Conditions

| condition | what |
| --- | --- |
| `A` | B5's winner, unchanged — the reference |
| `P` | `A` + contributor gate at `nonContributorFraction: 1.0` — the pass-through ablation, must equal `A` to the bit |
| `G` | `A` + strict contributor gate, 4-tick (2-character) window — arm 1 |
| `S` | `A` + soft bounds — arm 2 |
| `GS` | `A` + both — the interaction cell |
| `W2`…`W64` | `G` at 2, 8, 16, 32 and 64 ticks — Q2's window sweep, 15,000 characters only |

Seeds: the official ten (1–5, 11–15) at 15,000 where the verdicts live; three (1–3) at 200,000, where docs/decisions.md decision 28 says no accuracy verdict applies. Recorded in the script header rather than discovered here.

## Exactness controls — 23 comparisons, all identical

**X1, the pass-through ablation.** `nonContributorFraction: 1.0` gives a non-contributor the full delta, so the gated rule must reproduce the ungated one exactly. `P` matches `A` on **all ten seeds** in accuracy, `permanenceHash`, `weightHash` and `topologyHash`. This is arm 1's VAL-9 ablation and it cost no extra trials — it also proves the gate changes _which synapses are reached_ and nothing else.

**X2, `A` against C13's checkpointed rows.** `A` is B5's winner unchanged, already measured by `c13-permanence-trajectory-v1` on the same seeds. All **13** available comparisons (ten at 15,000, three at 200,000) are identical field for field, so the C14 core edit did not perturb the path it was not supposed to touch.

## The headline: both mechanisms worked, and VAL-4 did not move

### Q1 / Q3 / Q4 — the 2×2 at 15,000 characters, ten seeds

Paired per-seed deltas against `A`. Pre-registered: **HELPS** needs mean ≥ +1.0 point AND ≥ 8 of 10 seeds improving; **HURTS** needs mean ≤ −1.0; anything else is a **NULL**.

| condition | mean Δ | seeds better | worst seed | best seed | mean accuracy | vs 16.56% bar | verdict |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `G` | **+0.16** | 6/10 | −0.75 | +1.80 | 19.86% | +3.30 | **NULL** |
| `S` | **+0.10** | 2/10 | −0.15 | +1.10 | 19.80% | +3.25 | **NULL** |
| `GS` | **+0.02** | 5/10 | −0.70 | +1.25 | 19.73% | +3.17 | **NULL** |

`A`'s own mean is **19.71%**, +3.15 points over the bar. **Q4's interaction contrast** (`GS − G − S`, relative to `A`) is **−0.23 points**, inside the 1.0-point bar: the two arms add without interacting as far as this can resolve. There is no C9-shaped "each is a null alone but the pair does something" here.

### Q2 — the window sweep: the null is robust to the parameter, not an artefact of one

| window (ticks) | window (characters) | mean accuracy | mean Δ vs `A` | seeds better |
| --- | --- | --- | --- | --- |
| 2 | 1 | 19.73% | +0.03 | 3/10 |
| 4 | 2 | 19.86% | +0.16 | 6/10 |
| 8 | 4 | 19.82% | +0.12 | 4/10 |
| 16 | 8 | 19.73% | +0.03 | 5/10 |
| 32 | 16 | 19.63% | −0.08 | 3/10 |
| 64 | 32 | 19.64% | −0.07 | 2/10 |
| — (ungated) | — | **19.71%** | 0.00 | — |

**Across a 32-fold range of windows the spread is 0.23 points, and every row is within ±0.16 of the ungated reference.** This matters more than any single row: it forecloses "the window was wrong". There is no setting of this parameter at which contributor gating helps VAL-4, and none at which it hurts either.

### Q5 — 200,000 characters: the distribution moved a long way, the accuracy did not

| condition | mean accuracy @200k | mean Δ vs `A` | `atOne`/`occupied` | `mid`/`occupied` | `connected`/`occupied` |
| --- | --- | --- | --- | --- | --- |
| `A` | 15.32% | — | **55.1%** | **27.3%** | **70.2%** |
| `G` | 15.52% | +0.20 | **45.5%** | **37.4%** | **79.6%** |
| `S` | 14.57% | −0.75 | **0.0%** | **51.1%** | 70.6% |
| `GS` | 15.37% | +0.05 | **0.0%** | **51.0%** | 74.3% |

And the decline itself, by finding 26's peak-to-end statistic (5,000-character block peak, which is the block-level reading finding 26's own caveat insists on — hence 6.32 for `A` here against finding 26's 8.67, which used the raw 2,000-character sliding-window peak):

| condition | seed 1 | seed 2 | seed 3 | mean drop    |
| --------- | ------ | ------ | ------ | ------------ |
| `A`       | 4.18   | 6.40   | 8.39   | **6.32 pts** |
| `G`       | 6.09   | 4.00   | 7.47   | **5.85 pts** |
| `S`       | 7.00   | 4.31   | 9.89   | **7.06 pts** |
| `GS`      | 5.48   | 5.19   | 8.91   | **6.53 pts** |

**The between-condition spread is 1.2 points against a within-condition per-seed spread of 4 to 10.** Nothing here is an effect.

### Q6 — the one-way door did not bite, and the reason is the interesting part

| condition | `connected`/`occupied` @15k | @200k     |
| --------- | --------------------------- | --------- |
| `A`       | 100.0%                      | 70.2%     |
| `G`       | 98.7%                       | **79.6%** |
| `S`       | 100.0%                      | 70.6%     |
| `GS`      | 98.5%                       | 74.3%     |

The pre-registered worry was that a synapse below `connectionThreshold` never delivers, so under a strict gate it could never be reinforced back above it — a trapdoor the ungated rule does not have. **The opposite happened:** `G`'s connected fraction is **9.4 points higher** than `A`'s at 200,000, not lower.

The reason is visible in `mid`: gating raises the graded middle from 27.3% to 37.4% while lowering the ceiling pile from 55.1% to 45.5%. It de-polarises **both** ends at once, because concentrating reinforcement on contributors leaves fewer synapses being driven to the ceiling by coincidence _and_ leaves more of the reinforcement budget where it can hold a synapse above the gate. The trapdoor is real in principle and was measured not to dominate in practice — on this configuration, at this horizon.

**`DEFAULT_CONFIG` is the configuration where it would bite**, and a 3,000-character wiring smoke during development showed exactly that: strict gating took it from 16.00% to 4.70%. Its `connected/occupied` is ~13.6% from construction (docs/appendix/find-27.md), so most of its synapses never deliver and a strict gate starves it. That smoke is not a result — wrong horizon, one seed, and not the protocol's configuration — but it is why Q6 was pre-registered, and it says the trapdoor is a property of the _starting distribution_ rather than of the gate.

## What this settles

**The permanence-polarisation hypothesis is falsified as a cause of the post-peak decline.** docs/findings.md finding 27 could not order saturation against the decline, because learning and saturation overlap by construction, and said the way out was a perturbation rather than more measurement. This is that perturbation, and it is a strong one: two independent mechanisms, one upstream (which synapses get reinforced) and one downstream (how the update approaches the bounds), each comprehensively changed the distribution —

- `S` **eliminated ceiling saturation entirely** (55.1% → 0.0%) and **nearly doubled the graded middle** (27.3% → 51.1%);
- `G` cut the ceiling pile by 9.6 points, raised the middle by 10.1 and raised connectivity by 9.4;

— and **VAL-4 moved by at most 0.16 points at the pinned horizon, at any window, and the decline did not change.** A distribution that was supposed to be destroying graded discrimination was repaired, twice over, with no consequence for the thing it was supposed to be destroying.

## What this does not settle, stated plainly

- **The 200,000-character rows are three seeds with a ±2.5-point per-seed spread.** "The decline did not change" is well supported by the _distribution_ figures (which are tight and consistent) and weakly powered on accuracy. The ten-seed 15,000-character nulls are the statistically solid half.
- **It does not explain the decline.** It removes the leading candidate and leaves docs/open-questions.md item 7 open with one fewer hypothesis in it.
- **It does not say the mechanisms are worthless.** Both do exactly what the biology says they should, are measured to do it, and are kept in the tree behind default-off switches. What is measured is that on _this_ task, at _this_ scale, neither changes the outcome — which is the same shape as C6's and C9's results, and HANDOFF fact 12's standing caveat applies: VAL-4 is a stationary task and may simply not be able to see them.
- **`nonContributorFraction` between 0 and 1 was not swept.** The strict gate and the pass-through were both measured; the partial gate that Engert & Bonhoeffer (1997) and Harvey & Svoboda (2007) actually describe was not. Given that the strict gate is a null at every window, a partial one is very unlikely to be anything else, but it is untested and is recorded as such.

## Cost, for the next session's budgeting

112 trials in **61 minutes** on 12 workers: 12 trials at 200,000 characters (~40–44 min each, all in one wave) and 100 at 15,000 (~105 s each). The 200,000-character trials ran faster here than C13's 62–64 min because fewer of them shared the machine. Budget from concurrency, not from a per-trial figure.
