# Wider segmentsPerNeuron / fixed-targetRate verification -- segmentsPerNeuron=10

Generated 2026-09-13T18:10:57.441Z by scripts/verify-wider-segments-fixed-rates.ts 10.

Verification-only coarse spot-check, not a coordinate search (see this file's own module doc) -- checks 4 fixed targetRate values rather than searching to convergence. Official 5-seed protocol, 15,000-character corpus slice, matching Phase B (scripts/tune-segments-and-threshold.ts).

| segmentsPerNeuron | targetRate | seeds | mean network accuracy | range across seeds | mean trigram accuracy |
| --- | --- | --- | --- | --- | --- |
| 10 | 0.2500 | 5 | 3.99% | 3.35%-4.95% | 28.40% |
| 10 | 0.5000 | 5 | 12.26% | 11.50%-13.40% | 28.40% |
| 10 | 0.7500 | 5 | 10.76% | 9.55%-12.10% | 28.40% |
| 10 | 0.9900 | 5 | 11.49% | 7.45%-14.35% | 28.40% |
