# Wider segmentsPerNeuron / fixed-targetRate verification -- segmentsPerNeuron=12

Generated 2026-09-13T18:11:02.813Z by scripts/verify-wider-segments-fixed-rates.ts 12.

Verification-only coarse spot-check, not a coordinate search (see this file's own module doc) -- checks 4 fixed targetRate values rather than searching to convergence. Official 5-seed protocol, 15,000-character corpus slice, matching Phase B (scripts/tune-segments-and-threshold.ts).

| segmentsPerNeuron | targetRate | seeds | mean network accuracy | range across seeds | mean trigram accuracy |
| --- | --- | --- | --- | --- | --- |
| 12 | 0.2500 | 5 | 5.04% | 4.00%-6.70% | 28.40% |
| 12 | 0.5000 | 5 | 10.22% | 8.45%-12.40% | 28.40% |
| 12 | 0.7500 | 5 | 11.60% | 8.30%-15.25% | 28.40% |
| 12 | 0.9900 | 5 | 12.41% | 8.25%-15.65% | 28.40% |
