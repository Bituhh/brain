# B4 fix-parameter sweep -- second pass

Generated 2026-09-14T18:36:06.026Z by scripts/investigate-b4-fix-parameters.ts.
See that script's header for the design, stages and selection rule, and README
§12 decision 12 / §13.12 item 10 for what the results mean. 5-seed official
protocol (seeds 1-5, 15,000-character slice). Reference points from
investigate-structural-plasticity-drag.ts: condition C control 6.40%, sprout
disabled 16.51%, condition A (no structural plasticity) 17.37%.

## Stages 0-2 (sanity; each fix alone with weights frozen; STDP settings on condition A)

| condition                                                                               | mean network accuracy | per seed (1-5)                         | wall-clock (summed per-seed) |
| --------------------------------------------------------------------------------------- | --------------------- | -------------------------------------- | ---------------------------- |
| S0 sanity: condition C, every fix off                                                   | 6.40%                 | 13.10%, 3.50%, 3.95%, 3.75%, 7.70%     | 6089.9s                      |
| S1 fix 1 alone, frozen: unsilenceWeight=0.1                                             | 16.51%                | 14.00%, 17.60%, 18.35%, 17.50%, 15.10% | 305.6s                       |
| S1 fix 1 alone, frozen: unsilenceWeight=0.2                                             | 16.51%                | 14.00%, 17.60%, 18.35%, 17.50%, 15.10% | 338.4s                       |
| S1 fix 1 alone, frozen: unsilenceWeight=0.3                                             | 16.51%                | 14.00%, 17.60%, 18.35%, 17.50%, 15.10% | 347.2s                       |
| S1 fix 2 alone, frozen: window 1..2                                                     | 10.66%                | 10.05%, 11.65%, 10.95%, 13.55%, 7.10%  | 440.9s                       |
| S1 fix 2 alone, frozen: window 1..4                                                     | 11.36%                | 8.15%, 13.20%, 14.40%, 10.15%, 10.90%  | 433.2s                       |
| S1 fix 2 alone, frozen: window 1..8                                                     | 8.44%                 | 9.35%, 7.35%, 8.90%, 8.90%, 7.70%      | 525.6s                       |
| S1 fix 2 alone, frozen: window 1..16                                                    | 8.31%                 | 3.70%, 7.65%, 13.25%, 11.65%, 5.30%    | 613.1s                       |
| S1 fix 2 alone, frozen: window 1..64                                                    | 5.88%                 | 7.15%, 9.45%, 4.75%, 7.20%, 0.85%      | 784.5s                       |
| S1 fix 3 alone, frozen: segment spread on                                               | 3.88%                 | 1.50%, 2.35%, 3.15%, 9.40%, 3.00%      | 2879.8s                      |
| S1 fix 4 alone, frozen: eliminate after 400 ticks silent (silence tracked, not gated)   | 1.80%                 | 1.80%, 1.80%, 1.80%, 1.80%, 1.80%      | 11155.2s                     |
| S1 fix 4 alone, frozen: eliminate after 2000 ticks silent (silence tracked, not gated)  | 1.79%                 | 1.75%, 1.80%, 1.80%, 1.80%, 1.80%      | 9169.5s                      |
| S1 fix 4 alone, frozen: eliminate after 10000 ticks silent (silence tracked, not gated) | 1.86%                 | 1.90%, 1.85%, 1.80%, 2.05%, 1.70%      | 9756.2s                      |
| S2 condition A, weights frozen (sanity vs 17.37%)                                       | 17.37%                | 15.75%, 18.55%, 18.50%, 17.40%, 16.65% | 545.5s                       |
| S2 condition A + STDP: lr=0.02, tau=2                                                   | 17.37%                | 15.75%, 18.55%, 18.50%, 17.40%, 16.65% | 343.0s                       |
| S2 condition A + STDP: lr=0.1, tau=2                                                    | 17.37%                | 15.75%, 18.55%, 18.50%, 17.40%, 16.65% | 346.4s                       |
| S2 condition A + STDP: lr=0.5, tau=2                                                    | 17.37%                | 15.75%, 18.55%, 18.50%, 17.40%, 16.65% | 364.4s                       |
| S2 condition A + STDP: lr=0.02, tau=8                                                   | 17.37%                | 15.75%, 18.55%, 18.50%, 17.40%, 16.65% | 353.5s                       |
| S2 condition A + STDP: lr=0.1, tau=8                                                    | 17.37%                | 15.75%, 18.55%, 18.50%, 17.40%, 16.65% | 337.1s                       |
| S2 condition A + STDP: lr=0.5, tau=8                                                    | 17.37%                | 15.75%, 18.55%, 18.50%, 17.40%, 16.65% | 313.2s                       |

Batch wall-clock: 7588.1s.

Frozen-weight best window (fix 2 alone): 1..4. Frozen-weight best unsilence
weight (fix 1 alone): 0.1. Chosen STDP (best condition A): lr=0.02, tau=2.

## Stage 3 -- stopped twice, superseded by scripts/tune-b4-values.ts

**First attempt** (STDP chosen on condition A): stopped a few trials in. Stage 2
showed every STDP setting tying bit-for-bit on condition A, and a diagnostic run
(STDP at lr 0.5 changed ~13,800 synapse weights within 400 characters) showed
why -- internal synapses are all dendritic and segment votes ignore weight, so
choosing STDP there picked arbitrarily among ties.

**Second attempt** (stage 3a: STDP settings x unsilence weight, condition C with
the silent gate on): stopped after 11 trials, before any condition finished all
five seeds, because the design still (1) chose and reported on the same seeds,
(2) recorded nothing about what happened inside a trial, (3) chose each fix's
value alone before combining, and (4) hand-set the STDP grid. The trials that
did finish, from the run's console log (~20 minutes each):

| condition                                        | seeds finished | accuracy per seed                     | mean of finished seeds |
| ------------------------------------------------ | -------------- | ------------------------------------- | ---------------------- |
| fix 1 + STDP lr=0.02, tau=2: unsilenceWeight=0.1 | 1-5            | 14.05%, 13.50%, 13.30%, 7.40%, 13.80% | 12.41%                 |
| fix 1 + STDP lr=0.02, tau=2: unsilenceWeight=0.2 | 2-5            | 13.35%, 5.30%, 10.35%, 13.55%         | 10.64%                 |
| fix 1 + STDP lr=0.02, tau=2: unsilenceWeight=0.3 | 1-2            | 14.50%, 15.40%                        | 14.95%                 |

Read with care: partial, unconfirmed, and from one STDP setting. The one
complete row (12.41%) is below fix 1 alone with weights frozen (16.51%) -- at
this setting, letting sprouts unsilence cost accuracy rather than adding it.
That is a question for the full search, not a conclusion.
