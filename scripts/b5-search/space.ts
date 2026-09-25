// The parameter space PLAN.md B5's value search explores (docs/decisions.md
// decision 13, requirements.md Requirement 9.2): reference weight (with
// count mode as one of its levels), coincidence threshold, the STDP
// settings, the predictive-learning target, homeostatic scaling on/off, and
// B4's own four fix flags with their values -- every one of them changes
// meaning under weighted votes (requirements.md's own framing), so every
// one is searched together rather than carried over from B4's winner.

import { B4_PARAM_SPECS, type ParamSpec } from '../b4-search/space.ts';

export const B5_PARAM_NAMES = [
  // B5's own knobs.
  'voteReferenceWeight',
  'coincidenceThreshold',
  'predictiveLearningTarget',
  'homeostaticScaling',
  // Reused unchanged from B4_PARAM_SPECS (STDP + B4's four fixes and their values).
  'learningRate',
  'stdpTauTicks',
  'depressionRatio',
  'eligibilityTauTicks',
  'unsilenceWeight',
  'maxGapTicks',
  'eliminationTicks',
  'silentGate',
  'timingWindow',
  'spreadSegments',
  'silentElimination',
] as const;

export type B5ParamName = (typeof B5_PARAM_NAMES)[number];

/**
 * `0` is the sentinel for count mode (`DendriticVote::Count`) -- not a
 * valid weight (Requirement 1.6 requires `(0, 1]`), so it cannot collide
 * with a real level, and it sorts below every real weight, keeping the
 * level list ascending. `toConfig` (`conditions.ts`) reads it back as
 * "omit `voteReferenceWeight` entirely", not as a weight of zero. No
 * `extendDown`: count mode has no "lower than count" neighbour, and no
 * `extendUp`: `1.0` is already the reference weight's own hard bound.
 */
const VOTE_REFERENCE_WEIGHT_LEVELS = [
  0, 0.05, 0.1, 0.2, 0.35, 0.5, 0.65, 0.8, 1.0,
];

/**
 * Segment-threshold homeostasis is not in this space (docs/decisions.md decision 13 keeps that decided by B4's own default and only re-measures it as an
 * ablation, per requirements.md Requirement 6's homeostasis/silent-gate
 * framing being about weight *rescaling*, not threshold homeostasis).
 * `coincidenceThreshold` is fixed's own `BinaryCoincidenceParams::threshold`
 * -- meaning changes under weighted votes (an established synapse still
 * casts one full vote, so the same integer threshold means the same "this
 * many established synapses", but a *mixed* established/weak population now
 * reaches it differently than under count mode) -- searched, not assumed
 * unchanged from `charPrediction.ts`'s fixed `3`.
 */
export const B5_PARAM_SPECS: readonly ParamSpec<B5ParamName>[] = [
  { name: 'voteReferenceWeight', initial: VOTE_REFERENCE_WEIGHT_LEVELS },
  {
    name: 'coincidenceThreshold',
    initial: [1, 2, 3, 4, 5, 6],
    extendUp: (max) => (max + 1 <= 10 ? max + 1 : undefined),
  },
  // 0 = permanence, 1 = weight, 2 = both -- `conditions.ts`'s `learningTargetOf`.
  { name: 'predictiveLearningTarget', initial: [0, 1, 2] },
  // 0 = off, 1 = on.
  { name: 'homeostaticScaling', initial: [0, 1] },
  ...(B4_PARAM_SPECS as unknown as readonly ParamSpec<B5ParamName>[]),
];
