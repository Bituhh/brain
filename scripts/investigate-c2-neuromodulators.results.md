# PLAN.md C2 — driving neuromodulators from prediction error, measured

Generated 2026-09-20T14:24:31.347Z · corpus 15000 characters · 10 seeds · 6
conditions.

Reference is B5's winner exactly (README §12 decision 13): 19.05% on
confirmation seeds 11–15, 20.36% on selection seeds 1–5. Quote the **16.56%
"always guess space"** baseline (README §13.12 item 7) alongside any figure here
— a change that improves a delta but drops under that bar has undone the only
real progress the network has made.

### Inertness check

The no-consumer control reproduced the reference **exactly on every seed**, as
it must: the coupling wrote both channels every tick and nothing read them, so
not one spike could differ.

### Confirmation seeds (11–15)

| condition                                                                        | mean   | delta vs reference | per seed                           |
| -------------------------------------------------------------------------------- | ------ | ------------------ | ---------------------------------- |
| B5's winner (no coupling, the reference)                                         | 19.05% | —                  | 18.90% 18.10% 21.45% 19.30% 17.50% |
| NA gates predictive learning (LRN-8's reinforce/punish)                          | 19.28% | +0.23              | 18.95% 19.20% 21.45% 19.30% 17.50% |
| NA gates STDP (the three-factor weight update)                                   | 19.46% | +0.41              | 19.20% 18.80% 21.05% 19.55% 18.70% |
| NA gates both                                                                    | 19.46% | +0.41              | 19.25% 18.75% 21.05% 19.55% 18.70% |
| ACh driven by expected uncertainty, not held at 1.0                              | 20.46% | +1.41              | 21.30% 20.90% 20.45% 19.45% 20.20% |
| coupling on, no consumer (inertness control -- must equal the reference exactly) | 19.05% | +0.00              | 18.90% 18.10% 21.45% 19.30% 17.50% |

### Selection seeds (1–5)

| condition                                                                        | mean   | delta vs reference | per seed                           |
| -------------------------------------------------------------------------------- | ------ | ------------------ | ---------------------------------- |
| B5's winner (no coupling, the reference)                                         | 20.36% | —                  | 19.85% 20.50% 21.10% 21.15% 19.20% |
| NA gates predictive learning (LRN-8's reinforce/punish)                          | 20.36% | +0.00              | 19.85% 20.50% 21.10% 21.15% 19.20% |
| NA gates STDP (the three-factor weight update)                                   | 20.29% | -0.07              | 19.20% 20.60% 21.25% 20.80% 19.60% |
| NA gates both                                                                    | 20.29% | -0.07              | 19.20% 20.60% 21.25% 20.80% 19.60% |
| ACh driven by expected uncertainty, not held at 1.0                              | 19.66% | -0.70              | 19.10% 17.95% 21.40% 20.10% 19.75% |
| coupling on, no consumer (inertness control -- must equal the reference exactly) | 20.36% | +0.00              | 19.85% 20.50% 21.10% 21.15% 19.20% |

---

## Why noradrenaline did nothing: the signal itself, measured

The accuracy tables above cannot distinguish "the mechanism is inert here" from
"the mechanism is active and unhelpful". `investigate-c2-signal-shape.ts`
samples both derived signals every character (seed 7, 4,000 characters — an
instrumentation seed, deliberately outside the protocol's selection and
confirmation sets, since no accuracy claim is made from it):

| signal                                            | mean   | p50    | p90    | p99    | max    | exactly zero |
| ------------------------------------------------- | ------ | ------ | ------ | ------ | ------ | ------------ |
| surprise → noradrenaline (unexpected uncertainty) | 0.0004 | 0.0000 | 0.0002 | 0.0082 | 0.0141 | **89.5%**    |
| expected → acetylcholine (expected uncertainty)   | 0.4645 | 0.4396 | 0.5296 | 0.7364 | 0.8978 | 0.0%         |

**The noradrenaline channel is inert on this corpus, and that is a fact about
the task rather than about the mechanism.** Surprise is `max(0, fast − slow)`, a
_change_ detector. English prose has no contingency switches, so the fast and
slow estimates of the failure rate track each other and the difference is
exactly zero for nine characters in ten; at its most extreme it reaches 0.0141,
i.e. a gain of 1.014 against a baseline of 1.0. A multiplier that is exactly 1.0
for 90% of a run and within 1.4% of it otherwise cannot move an accuracy figure,
which is why the NA rows reproduce the reference _per seed identically_ on 5 of
5 selection seeds. This was pre-registered in the script header as the likely
outcome; it is recorded as a confirmed prediction, not a surprise.

**The acetylcholine channel is not inert** — median 0.44, ranging from about 0.2
to 0.9, never zero. Expected uncertainty is a real, substantial, varying signal
on this task, which is consistent with what the accuracy table shows: the ACh
row is the only one that moves VAL-4 at all, and it moves it by 1.41 points up
on confirmation seeds and 0.70 points down on selection seeds.

**Read that ACh result carefully: the two seed sets disagree by 2.1 points and
point in opposite directions.** By the standard C1 set for exactly this shape,
that is the honest description of _no effect_ — but it is a much larger
disagreement than the NA rows' and it is not noise-free. The correct reading is
that driving acetylcholine from expected uncertainty _does something_, and what
it does is not yet separable from seed variance at n=5. It is not adopted.
