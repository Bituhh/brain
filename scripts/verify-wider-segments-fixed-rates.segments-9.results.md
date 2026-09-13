# Wider segmentsPerNeuron / fixed-targetRate verification -- segmentsPerNeuron=9

Generated 2026-09-13T18:10:55.283Z by scripts/verify-wider-segments-fixed-rates.ts 9.

Verification-only coarse spot-check, not a coordinate search (see this file's own module doc) -- checks 4 fixed targetRate values rather than searching to convergence. Official 5-seed protocol, 15,000-character corpus slice, matching Phase B (scripts/tune-segments-and-threshold.ts).

| segmentsPerNeuron | targetRate | seeds | mean network accuracy | range across seeds | mean trigram accuracy |
|---|---|---|---|---|---|
| 9 | 0.2500 | 5 | 2.51% | 2.20%-2.95% | 28.40% |
| 9 | 0.5000 | 5 | 11.36% | 9.20%-13.05% | 28.40% |
| 9 | 0.7500 | 5 | 12.37% | 10.10%-14.75% | 28.40% |
| 9 | 0.9900 | 5 | 11.38% | 8.00%-15.95% | 28.40% |
