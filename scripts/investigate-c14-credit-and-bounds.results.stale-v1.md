# C14: contributor-gated reinforcement and soft-bound permanence updates

`investigate-c14-credit-and-bounds.ts`. Generated 2026-09-28 08:42:53 +0000. Protocol `c14-credit-and-bounds-v1`. Seeds 1, 2, 3, 4, 5, 11, 12, 13, 14, 15 (the official ten, VAL-6). **Every reading below was written into this script's header before any trial ran.** Raw per-trial series: `investigate-c14-credit-and-bounds.checkpoint.jsonl`.

- **A** — B5's winner, unchanged -- the reference (20.36% selection / 19.05% confirmation at 15,000)
- **P** — A + gate at `nonContributorFraction: 1.0` -- the pass-through ablation, must equal A to the bit (X1)
- **G** — A + strict contributor gate at 4 ticks (2 characters) -- arm 1
- **S** — A + soft bounds -- arm 2
- **GS** — A + strict gate + soft bounds -- the interaction cell
- **W2** — A + strict contributor gate at 2 ticks (1 characters) -- Q2's sweep
- **W8** — A + strict contributor gate at 8 ticks (4 characters) -- Q2's sweep
- **W16** — A + strict contributor gate at 16 ticks (8 characters) -- Q2's sweep
- **W32** — A + strict contributor gate at 32 ticks (16 characters) -- Q2's sweep
- **W64** — A + strict contributor gate at 64 ticks (32 characters) -- Q2's sweep

## Exactness controls

### X1 — the pass-through ablation: P must equal A to the bit

`nonContributorFraction: 1.0` gives a non-contributor the full delta, so the gated rule must reproduce the ungated one exactly. This is arm 1's VAL-9 ablation and it costs no extra trials.

| seed | chars  | A      | P      | permanenceHash | result           |
| ---- | ------ | ------ | ------ | -------------- | ---------------- |
| 1    | 15,000 | 19.85% | 19.85% | 56a1580d       | PASS — identical |
| 2    | 15,000 | 20.50% | 20.50% | 2bf55062       | PASS — identical |
| 3    | 15,000 | 21.10% | 21.10% | 92ea9475       | PASS — identical |
| 4    | 15,000 | 21.15% | 21.15% | 8fcaac5a       | PASS — identical |
| 5    | 15,000 | 19.20% | 19.20% | d18a7a9e       | PASS — identical |
| 11   | 15,000 | 18.90% | 18.90% | 49c5163f       | PASS — identical |
| 12   | 15,000 | 18.10% | 18.10% | 89603a1c       | PASS — identical |
| 13   | 15,000 | 21.45% | 21.45% | 1cab4b1b       | PASS — identical |
| 14   | 15,000 | 19.30% | 19.30% | 0552c60d       | PASS — identical |
| 15   | 15,000 | 17.50% | 17.50% | acf6152b       | PASS — identical |

### X2 — condition A against `c13-permanence-trajectory-v1`'s checkpointed rows

A is B5's winner unchanged, which C13 already measured on these same ten seeds at both horizons. Any difference would mean the C14 core edit changed behaviour it was not supposed to touch.

| seed | chars   | accuracy | permanenceHash | result           |
| ---- | ------- | -------- | -------------- | ---------------- |
| 1    | 200,000 | 17.00%   | fbecd053       | PASS — identical |
| 2    | 200,000 | 14.90%   | aebffb5b       | PASS — identical |
| 3    | 200,000 | 14.05%   | 9a11952c       | PASS — identical |
| 1    | 15,000  | 19.85%   | 56a1580d       | PASS — identical |
| 2    | 15,000  | 20.50%   | 2bf55062       | PASS — identical |
| 3    | 15,000  | 21.10%   | 92ea9475       | PASS — identical |
| 4    | 15,000  | 21.15%   | 8fcaac5a       | PASS — identical |
| 5    | 15,000  | 19.20%   | d18a7a9e       | PASS — identical |
| 11   | 15,000  | 18.90%   | 49c5163f       | PASS — identical |
| 12   | 15,000  | 18.10%   | 89603a1c       | PASS — identical |
| 13   | 15,000  | 21.45%   | 1cab4b1b       | PASS — identical |
| 14   | 15,000  | 19.30%   | 0552c60d       | PASS — identical |
| 15   | 15,000  | 17.50%   | acf6152b       | PASS — identical |

**All controls pass.**

## Q1, Q3, Q4 — the 2x2 at 15,000 characters (the pinned horizon)

Paired per-seed deltas against A, in accuracy points. Pre-registered: **HELPS** needs mean ≥ +1.0 AND ≥ 8 of 10 seeds improving; **HURTS** needs mean ≤ −1.0; anything else is a **NULL**.

| condition | mean Δ | seeds better | worst seed | best seed | mean accuracy | vs 16.56% bar | verdict |
| --- | --- | --- | --- | --- | --- | --- | --- |
| G | 0.16 | 6/10 | -0.75 | 1.80 | 19.86% | 3.30 pts | **NULL** |
| S | 0.10 | 2/10 | -0.15 | 1.10 | 19.80% | 3.25 pts | **NULL** |
| GS | 0.02 | 5/10 | -0.70 | 1.25 | 19.73% | 3.17 pts | **NULL** |

A's own mean at 15,000 is **19.71%** (3.15 points over the 16.56% "always guess space" bar).

**Q4, the interaction contrast** (GS − G − S, relative to A): **-0.23 points**. Inside the 1.0-point bar, so the two arms add without interacting as far as this can resolve.

## Q2 — the window sweep (15,000 characters, no verdict)

At `ticksPerInput` 2 a window of W ticks is W/2 characters. The ungated row is the asymptote a widening window should approach.

| window (ticks) | window (chars) | mean accuracy | mean Δ vs A | seeds better |
| -------------- | -------------- | ------------- | ----------- | ------------ |
| 2              | 1              | 19.73%        | 0.03        | 3/10         |
| 4              | 2              | 19.86%        | 0.16        | 6/10         |
| 8              | 4              | 19.82%        | 0.12        | 4/10         |
| 16             | 8              | 19.73%        | 0.03        | 5/10         |
| 32             | 16             | 19.63%        | -0.08       | 3/10         |
| 64             | 32             | 19.64%        | -0.07       | 2/10         |
| — (ungated)    | —              | 19.71%        | 0.00        | —            |

## Q5 — 200,000 characters: the decline, and whether saturation moved

No verdict on accuracy (decision 28: this horizon is for stability, not the milestone). **One pre-registered expectation:** if contributor gating does what it is designed to do, G's `atOne/occupied` must fall below A's band. If it does not, the mechanism did not do its job and no accuracy reading from it means anything.

| condition | mean accuracy @200k | mean Δ vs A | atOne/occupied | mid/occupied | connected/occupied |
| --- | --- | --- | --- | --- | --- |
| A | 15.32% | — | 55.1% | 27.3% | 70.2% |
| G | 15.52% | 0.20 | 45.5% | 37.4% | 79.6% |
| S | 14.57% | -0.75 | 0.0% | 51.1% | 70.6% |
| GS | 15.37% | 0.05 | 0.0% | 51.0% | 74.3% |

## Q6 — the one-way door: `connected/occupied` at both horizons

A synapse below `connectionThreshold` never delivers, so under a strict gate it can never be reinforced back above it. If G's connected fraction collapses relative to A, that is this trapdoor — and it is an argument for a non-zero `nonContributorFraction`, not against gating.

| condition | connected/occupied @15k | @200k |
| --------- | ----------------------- | ----- |
| A         | 100.0%                  | 70.2% |
| G         | 98.7%                   | 79.6% |
| S         | 100.0%                  | 70.6% |
| GS        | 98.5%                   | 74.3% |
