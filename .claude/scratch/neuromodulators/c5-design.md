# PLAN.md C5 — modulators reach `StdpParams`: design proposal

Written 2026-09-21, **before** the implementation, per task step 2 ("propose
before implementing"). Measured numbers are appended at the bottom as they
arrive; nothing above the `## Measured` heading is edited after the fact except
to correct it, visibly.

## 0. A premise in the prompt that is not true, and why it changes the cost question

Task step 3 says "the existing design precomputes decay constants exactly to
keep [`kernel`] cheap. A per-event `exp()` because a time constant became
dynamic is a real regression."

`StdpParams::kernel` does **not** precompute anything. It already evaluates
`a_plus * (-dt / tau_plus).exp()` — one division and one `exp()` — **per
event**, today. The precomputed constants are
`ThreeFactorParams::eligibility_decay_per_tick` and `LifParams::decay_per_tick`,
which are different quantities on different code paths and are not touched here.
So making `tau_plus` modulator-dependent replaces `dt / tau` with
`dt / (tau * scale)`: **no new transcendental**, one extra multiply. The
quantise-and-cache fallback the prompt anticipates has nothing to cache. This is
still _measured_ (a claim about cost is not a measurement), but the expectation
to be checked is "indistinguishable from noise", not "a real regression".

## 1. Shape of the API

`StdpParams` keeps its five constants and stays a plain `Copy` struct built by
literal at ~20 call sites. **It is not changed**; adding fields would touch
every one of them for no benefit. The modulation lives beside it, and attaches
to `ThreeFactorParams` exactly the way C2's `gain_modulator_index` did (a
builder, `None` from `new`):

- `LevelMap { channel, reference, gain, min, max }` — one quantity's mapping
  from a broadcast level to a dimensionless scale.
- `StdpModulation { a_plus, a_minus, tau_plus, tau_minus, window_ticks: Option<LevelMap> }`
  — the prompt's "an `Option<usize>` per modulated quantity", with the channel
  carried inside the map. `None` in a slot means "use the configured constant".
- `ThreeFactorParams::stdp_modulation: Option<StdpModulation>`; `None` (what
  `new` sets, so every existing caller) takes the _unchanged_
  `StdpParams::kernel`. Not "multiply by 1.0 and hope": the unset path is the
  same instruction sequence as before, and the modulated path is a separate
  function, `StdpParams::kernel_modulated`, that reduces to `kernel` when every
  scale is 1.0.

Five independent slots rather than one "shape" knob, because the three claims
C6/C7/F19 will make are different claims (§2) and a single knob would force them
to be the same one.

## 2. How a level maps onto each quantity

**Common form: `scale = clamp(1 + gain * (level - reference), min, max)`;
quantity = constant × scale.**

Not `quantity = constant × level`, which is what both existing consumers do, for
two reasons that are specific to a curve _shape_ rather than a delta:

1. **A shape has no meaningful zero.** `delta × 0` is "no learning this tick",
   which is a fine thing for a gate to mean. `window × 0` is "no pairing
   counts", `tau × 0` is a division by zero. C2 measured noradrenaline exactly 0
   for 89.5% of a VAL-4 run, so a raw multiplier would make the _resting_ state
   a degenerate curve. **[Corrected 2026-09-21, post-close review]** The 89.5%
   is the surprise _signal_ (4,000 characters, seed 7, `DEFAULT_CONFIG`), not
   the level. The level is `baseline + gain × surprise`, clamped, and on B5's
   configuration it rests at 0.9991
   (`scripts/investigate-c5-horizon.results.md`, Q5). The example is wrong; the
   point about shapes having no meaningful zero stands, and point 2 is the
   stronger reason for the reference.
2. **The biology is stated relative to a resting state.** "β-adrenergic
   activation widened the window by ~15 ms", "M1 activation converts LTP to LTD"
   are both changes _from_ the curve without the modulator. `reference` is the
   level at which the configured constant holds, so the configured `StdpParams`
   keep meaning "the resting curve".

Consequence, and the property the tests pin: **at `level == reference` the scale
is exactly 1.0 and the modulated kernel is bit-identical to the plain one**
(`x * 1.0` is exact). That is the analogue of C3's "baseline 1.0 reproduces the
unmodulated rule", and it is what makes a measured difference between "hook on"
and "hook unset" attributable to the level varying rather than to a change of
scale.

Per quantity:

| quantity                | claim a scale makes                 | range                             | notes                                                                                                                                                                                                                                                                                                                                       |
| ----------------------- | ----------------------------------- | --------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `a_plus`, `a_minus`     | how much a pairing counts, per side | **caller's**, may cross zero      | independent slots so the ratio can move by either. A negative scale on `a_minus` is a _sign inversion_ (anti-causal pairings potentiate) — the triangular-window and ACh-inverts-the-sign claims both need it, and whether to allow it is C6's/C7's decision, so C5 permits it only if the caller's `min` is negative and defaults nothing. |
| `tau_plus`, `tau_minus` | how long a pairing's credit lasts   | **strictly positive** (validated) | also changes the kernel's _area_ (∫ = a·τ), not just its width. That is a different claim from the window.                                                                                                                                                                                                                                  |
| `window_ticks`          | _which pairings count at all_       | strictly positive                 | see §3.                                                                                                                                                                                                                                                                                                                                     |

**Trap worth writing down for C6:** scaling `tau` alone is _capped by the
window_ — a kernel whose tail is cut at `window_ticks` cannot get wider than the
window however large tau grows, so "widen the window" implemented as tau-only is
silently invisible past the cutoff. For that reason
`StdpModulation::joint_time_scale` sets tau_plus, tau_minus and window from one
map. The five slots stay independent for anyone who really wants a different
claim.

## 3. `window_ticks`: no rounding, and the staircase it does have

The prompt says rounding an integer window per event "has a cost and a
discontinuity". The cost is avoidable: `dt` is already an `f32` and the test is
`dt.abs() > window as f32`, so the modulated form is
`dt.abs() > window as f32 * scale` — **a float compare, no per-event rounding,
no `round()` call.** The discontinuity is not avoidable and is inherent, not
introduced: `dt` is an _integer_ tick difference, so the effective bound is
`floor(window × scale)` and the set of pairings that count changes only when
`window × scale` crosses an integer. That is a staircase in `scale` with treads
`1/window` wide (1/40 = 0.025 at the shipped `windowTicks: 40`). **[Corrected
2026-09-21]** B5's winner runs `windowTicks: 20` and τ = 4 (40/8 is the search's
base in `scripts/b5-search/conditions.ts`), so the treads are 1/20 = 0.05 wide.
The 5τ edge figure below is unchanged.

How bad the _jump_ at each edge is depends on how much kernel mass is sitting at
the edge: `a · exp(-window·scale / (tau·scale_tau))`. With the window and tau
scaled **together** that ratio is constant (`exp(-window/tau)`, 0.7% of `a` at
the shipped 5τ), so the joint scale is a staircase whose treads are 0.7% of the
amplitude tall — negligible. With the window scaled alone the edge moves through
the exponential and the jump changes with the scale. This is the cheap, local
answer to "is the window a continuous knob"; task step 5 is the question about
the _whole system_, and is measured separately.

## 4. When the level is read

At **event time**, inside the `kernel` call — the curve that applies to a
pairing is the curve at the moment the pairing is evaluated, and the
contribution is _stored in eligibility_, not re-scaled when the level later
changes. This is the biologically natural reading (the modulator present during
induction shapes the induction), and it differs from the existing multiplicative
gate, which reads the level at _apply_ time and can therefore act on eligibility
that was laid down under a different level. Both are recorded because a later
reader will otherwise assume the hook gates eligibility retroactively.

## 5. What this item does NOT do

No channel is wired to anything in `canonicalBrain.ts` or `charPrediction.ts`'s
defaults; the hook ships unset everywhere. C6 wires noradrenaline to the joint
time scale, C7 wires acetylcholine to the `a_minus` slot, F19 serotonin to the
ratio. The FFI carries the option (so C6/C7 can measure VAL-4, and so the FFI
does not become "a copy that stops being a copy", HANDOFF fact 14(a)) and
nothing in the TypeScript shell sets it.

---

## Measured (appended 2026-09-21, after implementation)

### Hot-path cost (task step 3, ENG-9)

Expectation from §0 was "indistinguishable from noise". The result is more
nuanced, and one comparison in it was a mistake I caught rather than reported.

**Benchmarks**, `crates/brain-core/benches/core_bench.rs`, criterion, idle
machine, release, `stdp_*` groups; raw output `c5-bench-run1.txt`. The hook
variants are all measured at a level OFF the reference so no scale is 1.0 and
nothing folds away.

| level                                                     | unset | `a_minus` only | joint time scale | all five slots |
| --------------------------------------------------------- | ----- | -------------- | ---------------- | -------------- |
| bare `kernel` / `kernel_modulated`, ns per event          | 4.0   | 9.4            | 10.0             | 14.3           |
| `ThreeFactorStdp::on_post_spike`, ns per event            | 29.0  | 28.5           | 29.1             | 30.7           |
| in-situ, 3,200-neuron plasticity network, ms per 50 ticks | 14.66 | 14.79          | 14.92            | 14.76          |

- **The bare curve is 2.3-3.6x slower** (+5 to +10 ns per event on a 4 ns
  baseline), so the "one extra multiply" expectation was optimistic at the level
  of the arithmetic alone. Five `LevelMap::scale` evaluations (bounds-checked
  channel load, sub, mul, add, NaN-safe max/min) are not free, and the plain
  kernel is only 4 ns because it has one `exp()` and predictable structure.
- **At the rule level that is diluted to 0 to +6%** (`stdp_rule_per_event`:
  eligibility decay, `powi`, the kernel, the weight update, per event), worst
  case all five slots mapped.
- **The in-situ row cannot distinguish anything, and I am not presenting it as
  evidence the hook is free.** That fixture holds only **4,387 kernel-evaluating
  STDP events per iteration** (3,920 spikes -> 2,194 deliveries + 2,193
  post-spike callbacks), ~0.9% of its 14.6 ms, so even a +50% kernel would read
  as +0.4%. It is kept because the constraint is "a caller who does not opt in
  pays nothing" and it shows no regression, not because it measures the hook.
- **The real workload is the measurement that means something:**
  `scripts/measure-c5-hook-cost.ts`, the 800-neuron VAL-4 network, B5's winner,
  1,500 characters, 5 alternating repetitions after a discarded warm-up pair,
  hook set on all five slots **at its reference level** so the event stream is
  identical to the unset run: **1.286 s -> 1.317 s, +2.4%.** The two arms'
  repetitions do not overlap (unset 1.280-1.314, set 1.315-1.330). That is the
  worst case; early in a run, at low synapse counts, so the STDP event share is
  if anything lower than it will be later.
- **Bit-identity at the reference level, end to end through the FFI on that
  network: PASS** (permanence, weight and topology hashes and accuracy all
  equal).

**The unset path costs nothing (Requirement 5.2's cost half), and how I nearly
got that wrong.** The first comparison put the new build's `unset` kernel (16.5
us) against a pre-C5 baseline (13.6 us) and would have read as a 21% regression.
It was not: the new bench wraps the plain loop in a `match` over the variants,
the baseline did not. Re-running the _identical baseline source_ against both
builds (pre-C5 = a detached worktree at 34bd89a): kernel 13.65 / 13.62 us (new)
vs 13.62 / 13.52 / 13.48 us (old); `on_post_spike` 111.0 / 111.3 us (new) vs
119.0 / 118.2 / 118.8 us (old). So the new library's unset path is **not
slower**; the rule-level number is if anything ~6% _faster_, which I attribute
to code layout and do not claim as a benefit.

**Decision: no quantise-and-cache.** The fallback in step 3 exists for a
per-event `exp()` that became dynamic; there is none, `exp()` was already per
event. What would be worth doing _if_ a profile ever shows this: the scales are
constant across a whole tick (the level is a broadcast), so they could be
hoisted to once per tick per rule instead of once per event. Not built: it needs
the rule to know tick boundaries, `PlasticityRule` takes `&self` and is `Sync`,
and 2.4% of a run with the worst-case configuration is not a cost anyone has
seen matter.

### Post-close review (2026-09-21): the 6,000-character conclusions at 15,000

`scripts/investigate-c5-horizon.ts` re-checked what this item measured only at
6,000 characters. Summary (full account in README §13.12 item 18's addendum):
the weight path is _sensitive_, not continuous, at 15,000 (a 1e-4 nudge moves
topology on one seed; a 1e-3 nudge moves accuracy up to 0.40 points); the joint
time scale's narrowing side reversed with horizon and nothing beats the shipped
window; its effect is mainly _width_, not area; the permanence path is _nearly_
inert; and noradrenaline's signal is almost absent after the first third of a
run, with its level resting at 0.9991 rather than the drive's baseline of 1.0.

### Things this item's own scripts got wrong first

- `measure-c5-hook-cost.ts` reported a **weight-hash mismatch** between "hook
  unset" and "hook set at its reference". That was the experiment, not the hook:
  the harness holds a tonic level by topping it up once per character, so
  between top-ups the level has decayed by a few tenths of a percent, the scale
  is 0.998 rather than 1.0, and the runs legitimately differ. Fixed by giving
  noradrenaline an effectively infinite decay constant in both arms
  (`exp(-1/1e30)` is exactly 1.0 in f32). The Rust test suite avoids the same
  trap the same way (`NO_DECAY`).
