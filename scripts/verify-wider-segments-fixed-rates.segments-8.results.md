# Wider segmentsPerNeuron / fixed-targetRate verification -- segmentsPerNeuron=8

Generated 2026-09-13T18:10:53.055Z by scripts/verify-wider-segments-fixed-rates.ts 8.

Verification-only coarse spot-check, not a coordinate search (see this file's own module doc) -- checks 4 fixed targetRate values rather than searching to convergence. Official 5-seed protocol, 15,000-character corpus slice, matching Phase B (scripts/tune-segments-and-threshold.ts).

| segmentsPerNeuron | targetRate | seeds | mean network accuracy | range across seeds | mean trigram accuracy |
| --- | --- | --- | --- | --- | --- |
| 8 | 0.2500 | 5 | 3.00% | 2.05%-4.30% | 28.40% |
| 8 | 0.5000 | 5 | 11.55% | 10.65%-13.10% | 28.40% |
| 8 | 0.7500 | 5 | 13.08% | 11.05%-15.10% | 28.40% |
| 8 | 0.9900 | 5 | 13.62% | 10.45%-16.35% | 28.40% |
