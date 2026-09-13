# Wider segmentsPerNeuron / fixed-targetRate verification -- segmentsPerNeuron=6

Generated 2026-09-13T18:10:48.206Z by scripts/verify-wider-segments-fixed-rates.ts 6.

Verification-only coarse spot-check, not a coordinate search (see this file's own module doc) -- checks 4 fixed targetRate values rather than searching to convergence. Official 5-seed protocol, 15,000-character corpus slice, matching Phase B (scripts/tune-segments-and-threshold.ts).

| segmentsPerNeuron | targetRate | seeds | mean network accuracy | range across seeds | mean trigram accuracy |
|---|---|---|---|---|---|
| 6 | 0.2500 | 5 | 2.22% | 1.00%-2.75% | 28.40% |
| 6 | 0.5000 | 5 | 8.53% | 4.90%-11.05% | 28.40% |
| 6 | 0.7500 | 5 | 13.97% | 12.70%-15.05% | 28.40% |
| 6 | 0.9900 | 5 | 15.42% | 12.10%-16.80% | 28.40% |
