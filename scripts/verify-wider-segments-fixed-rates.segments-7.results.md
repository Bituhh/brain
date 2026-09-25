# Wider segmentsPerNeuron / fixed-targetRate verification -- segmentsPerNeuron=7

Generated 2026-09-13T18:10:50.357Z by
scripts/verify-wider-segments-fixed-rates.ts 7.

Verification-only coarse spot-check, not a coordinate search (see this file's
own module doc) -- checks 4 fixed targetRate values rather than searching to
convergence. Official 5-seed protocol, 15,000-character corpus slice, matching
Phase B (scripts/tune-segments-and-threshold.ts).

| segmentsPerNeuron | targetRate | seeds | mean network accuracy | range across seeds | mean trigram accuracy |
| ----------------- | ---------- | ----- | --------------------- | ------------------ | --------------------- |
| 7                 | 0.2500     | 5     | 2.28%                 | 1.80%-2.65%        | 28.40%                |
| 7                 | 0.5000     | 5     | 9.89%                 | 8.90%-11.40%       | 28.40%                |
| 7                 | 0.7500     | 5     | 13.16%                | 11.85%-14.35%      | 28.40%                |
| 7                 | 0.9900     | 5     | 15.29%                | 13.20%-18.05%      | 28.40%                |
