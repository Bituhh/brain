# Wider segmentsPerNeuron / fixed-targetRate verification -- segmentsPerNeuron=1

Generated 2026-09-13T18:10:24.774Z by scripts/verify-wider-segments-fixed-rates.ts 1.

Verification-only coarse spot-check, not a coordinate search (see this file's own module doc) -- checks 4 fixed targetRate values rather than searching to convergence. Official 5-seed protocol, 15,000-character corpus slice, matching Phase B (scripts/tune-segments-and-threshold.ts).

| segmentsPerNeuron | targetRate | seeds | mean network accuracy | range across seeds | mean trigram accuracy |
| --- | --- | --- | --- | --- | --- |
| 1 | 0.2500 | 5 | 5.86% | 4.70%-7.75% | 28.40% |
| 1 | 0.5000 | 5 | 8.05% | 6.95%-9.40% | 28.40% |
| 1 | 0.7500 | 5 | 11.90% | 9.35%-14.10% | 28.40% |
| 1 | 0.9900 | 5 | 16.65% | 16.65%-16.65% | 28.40% |
