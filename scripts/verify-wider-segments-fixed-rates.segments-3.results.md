# Wider segmentsPerNeuron / fixed-targetRate verification -- segmentsPerNeuron=3

Generated 2026-09-13T18:10:32.502Z by scripts/verify-wider-segments-fixed-rates.ts 3.

Verification-only coarse spot-check, not a coordinate search (see this file's own module doc) -- checks 4 fixed targetRate values rather than searching to convergence. Official 5-seed protocol, 15,000-character corpus slice, matching Phase B (scripts/tune-segments-and-threshold.ts).

| segmentsPerNeuron | targetRate | seeds | mean network accuracy | range across seeds | mean trigram accuracy |
| --- | --- | --- | --- | --- | --- |
| 3 | 0.2500 | 5 | 2.99% | 2.00%-3.95% | 28.40% |
| 3 | 0.5000 | 5 | 4.43% | 3.20%-6.60% | 28.40% |
| 3 | 0.7500 | 5 | 7.70% | 3.95%-10.85% | 28.40% |
| 3 | 0.9900 | 5 | 15.27% | 14.25%-16.25% | 28.40% |
