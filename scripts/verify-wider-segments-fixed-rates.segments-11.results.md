# Wider segmentsPerNeuron / fixed-targetRate verification -- segmentsPerNeuron=11

Generated 2026-09-13T18:10:59.779Z by
scripts/verify-wider-segments-fixed-rates.ts 11.

Verification-only coarse spot-check, not a coordinate search (see this file's
own module doc) -- checks 4 fixed targetRate values rather than searching to
convergence. Official 5-seed protocol, 15,000-character corpus slice, matching
Phase B (scripts/tune-segments-and-threshold.ts).

| segmentsPerNeuron | targetRate | seeds | mean network accuracy | range across seeds | mean trigram accuracy |
| ----------------- | ---------- | ----- | --------------------- | ------------------ | --------------------- |
| 11                | 0.2500     | 5     | 4.61%                 | 3.70%-5.65%        | 28.40%                |
| 11                | 0.5000     | 5     | 9.38%                 | 6.95%-11.05%       | 28.40%                |
| 11                | 0.7500     | 5     | 11.18%                | 8.95%-12.85%       | 28.40%                |
| 11                | 0.9900     | 5     | 12.68%                | 9.50%-15.50%       | 28.40%                |
