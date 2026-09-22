# PLAN.md C7 — acetylcholine sets the LTP/LTD ratio: the design calls

Written 2026-09-22, before the VAL-4 battery ran. Both calls were put to the user, who asked for the
biological evidence first and then chose on it. README §12 decision 18 is the canonical record; this
file keeps the working that led to it.

## What acetylcholine does on VAL-4 (measured first)

`scripts/investigate-c7-ach-level.ts`, six seeds, C2's coupling on acetylcholine (tau 100/2000,
baseline 1.0, gain 1.0), shared wiring, a gain-0 `aPlus` map observing:

| | first third | middle third | last third |
|---|---|---|---|
| median level (seeds 1, 2, 3, 11, 12, 13) | 1.85–1.86 | 1.46–1.49 | 1.32–1.35 |
| last-third 5–95% spread | | | ~0.05 |

Starts at 1.00 (seeded), peaks ~1.97 within a few hundred characters, falls slowly. **A learning-
progress schedule, not a fluctuating signal.** A ratio map from it is, on this task, a depression-
heavy start that relaxes as the network learns. C5 found early dynamics decide the long run (weak
depression early buys early accuracy and costs the long run), so this is a live hypothesis rather
than a formality.

Also measured on the way: **C2's recorded "ACh driven" row does not reproduce at HEAD** (seed 1
19.70% against 19.10% recorded; seed 11 20.55% against 21.30%), while B5's own reference does
(19.85%), and a gain-0 map equals no hook exactly. A rebuild of C2's commit in a detached worktree
did not reproduce B5's reference either (22.05%), so that environment was not trusted and the cause
is left unidentified. The battery re-measures the row fresh (arm V) rather than reading C2's
checkpoint.

## Call 1 — may the ratio cross zero? Yes, graded, floor −1, with a floor-0 twin

Evidence (primary papers, checked 2026-09-22):

- **For inversion.** Seol et al. 2007 (visual cortex, M1 agonist): LTD "regardless of the order of
  pre- and postsynaptic activation". Brzosko et al. 2017 (mouse CA1, 1 µM ACh, muscarinic, blocked by
  atropine): a +10 ms pairing went from t-LTP 135 ± 7% to t-LTD 63 ± 8%; post-before-pre also LTD;
  narrow window (0 and −20 ms, not ±50). **Dose:** at 100 nM ACh "prevented significant
  potentiation" without depression — suppression at low tone, inversion at high.
- **Against / qualifying.** Sugisaki et al. 2011 (rat CA1, muscarinic): the *opposite* — LTP
  facilitated, LTD switched to LTP; excess ACh abolished all STDP. Auditory cortex L2/3 (bioRxiv
  690446, 2019): muscarinic activation suppressed t-LTP, no inversion reported, not via M1/M3.
  Gu & Yakel 2011: the outcome depends on the timing of cholinergic input relative to glutamatergic
  input and on receptor (α7 nicotinic vs muscarinic). Brzosko 2017's own caveat: polarity "can depend
  on the concentration of agonist used and specific cholinergic receptor subtype activated".

Decision: the best-controlled STDP-protocol results (two labs, two areas) support inversion at high
muscarinic tone and suppression at low tone, which is exactly an affine map through zero continuing
below it. So `aPlus` map, negative gain, `max` 1.0 (never enhances LTP), **`min` −1** (Brzosko's
+35% → −37%: the inverted side about as strong as the configured LTP). `aMinus` not mapped — the
anti-causal side is already LTD, and a second gain is a free parameter no result pins.

The twin (`min` 0) stays, as a measurement control rather than a design alternative: in Brzosko's
account dopamine arriving within minutes converts the ACh-driven LTD back into LTP, and here
dopamine writes permanence (C3), so the rescue does not exist. The twin measures whether inversion
*without* its rescue helps or hurts. Known risk it would expose: a runaway loop (high uncertainty →
causal learning becomes forgetting → uncertainty stays high).

## Call 2 — which of acetylcholine's two roles? Induction only

Brzosko 2017: "acetylcholine did not have an effect on plasticity when applied after the induction
protocol", and did not change baseline strength without pairing; the retroactive factor that converts
a tag minutes later is dopamine. So in biology acetylcholine shapes the rule *at induction* and does
not multiply the cash-in. C5's hook reads the level at event time and stores it in eligibility —
exactly that locus. B5's shipped config routes the three-factor cash-in on acetylcholine, harmless
while it was a constant 1.0, not biology once it varies (a 33–97% learning-rate change on VAL-4).

Decision: the primary arms route the cash-in on serotonin (channel 3, unread by anything) held at
1.0 by `tonicModulator` — B5's hold moved to a neutral channel, a "no modulator at cash-in", not a
claim about serotonin — and acetylcholine reaches only the ratio. The shipped wiring stays as the
prompt's comparison (arms V and SHARED3). An exactness control (G) checks the moved routing is
bit-identical to B5.

## Reference

The level a pairing reads once the network has learned what it can. In the Rust mechanism test the
network learns the sequence perfectly, so that is zero-uncertainty rest (0.957, measured by a gain-0
map's `minLevel`). On VAL-4 expected uncertainty never approaches zero, so it is the late-run level:
median of the selection seeds' last-third medians, × exp(−1/1000) for the one tick of decay between
the between-character sample and the pairing that reads it. Pre-registered in the script header.

## Instrument

`StdpModulationStats::amplitude_inverted` (FFI `amplitudeInverted`): pairings inside the window whose
own side's amplitude scale was negative — the kernel's sign actually flipped. Reported beside
accuracy.

## Mechanism test and ablation

`tests/prediction_error_coupling.rs`, C7 section: a causal probe pairing inside the resting window
rides along C2's A→B learning scenario. Naive (acetylcholine high): the same pairing lays down
depression and the synapse weakens; learned: back to the configured LTP. Floor-0 twin: suppressed to
exactly 0, never negative, weight never falls. VAL-9: acetylcholine held *exactly* (non-decaying
field, one injection — the coupling at drive gain 0 is not an exact hold: pairings read 1.0 or one
tick of decay below depending on where in a tick they fall) at the reference gives the configured
LTP on every exposure and a run bit-identical to "hook unset, acetylcholine varying"; held away from
the reference, a constant retune. The ablation disables the hook's event-time read; the cash-in is on
held serotonin throughout. Sabotaging `kernel_modulated`'s `a_plus` scaling fails all three tests.
