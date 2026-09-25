# Wider segmentsPerNeuron / fixed-targetRate verification -- segmentsPerNeuron=5

Generated 2026-09-13T18:10:40.076Z by scripts/verify-wider-segments-fixed-rates.ts 5.

Verification-only coarse spot-check, not a coordinate search (see this file's own module doc) -- checks 4 fixed targetRate values rather than searching to convergence. Official 5-seed protocol, 15,000-character corpus slice, matching Phase B (scripts/tune-segments-and-threshold.ts).

| segmentsPerNeuron | targetRate | seeds | mean network accuracy | range across seeds | mean trigram accuracy |
| --- | --- | --- | --- | --- | --- |
| 5 | 0.2500 | 5 | 2.55% | 1.40%-3.10% | 28.40% |
| 5 | 0.5000 | 5 | 6.35% | 4.15%-7.85% | 28.40% |
| 5 | 0.7500 | 5 | 12.73% | 12.00%-13.70% | 28.40% |
| 5 | 0.9900 | 5 | 15.71% | 13.70%-16.80% | 28.40% |
