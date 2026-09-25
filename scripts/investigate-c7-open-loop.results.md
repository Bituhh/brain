# PLAN.md C7 -- open-loop diagnostic (post hoc, not pre-registered; no verdict is drawn from it)

Generated 2026-09-22T07:07:31.053Z by `scripts/investigate-c7-open-loop.ts` (its
header has the question and the mechanics). 15,000 characters, B5 winner base,
INV3 map (g 3, floor -1, reference 1.4565618470117512), cash-in on held
serotonin.

- **record** = the battery M row (ACh driven, nothing reads it; identical to
  B5). Its per-character ACh trajectory is saved.
- **closed** = the battery INV3 row, re-run (ACh driven by the network the map
  is acting on).
- **open** = the INV3 map, no coupling, ACh replayed from that seed own record
  trajectory.

| seed     | record (= B5) | closed (INV3) | open (replayed ACh) | ACh median by third: closed | open                     | inverted share: closed / open |
| -------- | ------------- | ------------- | ------------------- | --------------------------- | ------------------------ | ----------------------------- |
| 1        | 19.85%        | 1.25%         | 2.25%               | 1.9253 / 1.8118 / 1.77      | 1.9165 / 1.6917 / 1.458  | 13.76% / 8.78%                |
| 2        | 20.50%        | 0.50%         | 2.90%               | 1.9279 / 1.8181 / 1.7732    | 1.9274 / 1.7129 / 1.4639 | 13.83% / 9.69%                |
| 3        | 21.10%        | 0.90%         | 2.20%               | 1.9163 / 1.7835 / 1.7441    | 1.9135 / 1.7117 / 1.449  | 11.32% / 8.79%                |
| 4        | 21.15%        | 1.20%         | 3.10%               | 1.9171 / 1.7995 / 1.7559    | 1.9142 / 1.6788 / 1.4504 | 12.92% / 8.07%                |
| 5        | 19.20%        | 1.45%         | 1.65%               | 1.92 / 1.7859 / 1.749       | 1.9153 / 1.7042 / 1.4794 | 11.62% / 8.92%                |
| 11       | 18.90%        | 0.80%         | 3.70%               | 1.9339 / 1.8332 / 1.7858    | 1.9159 / 1.7013 / 1.4289 | 16.45% / 9.26%                |
| 12       | 18.10%        | 3.85%         | 5.60%               | 1.9323 / 1.8045 / 1.742     | 1.9156 / 1.6914 / 1.4298 | 12.90% / 8.52%                |
| 13       | 21.45%        | 1.75%         | 1.85%               | 1.9245 / 1.7911 / 1.7581    | 1.9096 / 1.711 / 1.467   | 12.02% / 8.98%                |
| 14       | 19.30%        | 1.35%         | 3.70%               | 1.9317 / 1.798 / 1.7638     | 1.9188 / 1.7006 / 1.4343 | 12.73% / 8.93%                |
| 15       | 17.50%        | 0.70%         | 1.25%               | 1.9275 / 1.8082 / 1.77      | 1.9277 / 1.7015 / 1.4643 | 13.04% / 9.23%                |
| **mean** | 19.71%        | 1.38%         | 2.82%               |                             |                          |                               |

Network prediction meter (`predictionAccuracy()`, the substrate own smoothed
rate -- a shape, not the protocol figure), every 1,000 characters, seed 1:

- record: 0.0006, 0.0239, 0.1071, 0.1797, 0.232, 0.3185, 0.3654, 0.4247, 0.4793,
  0.5564, 0.5021, 0.6199, 0.6207, 0.7123
- closed: 0.0006, 0.0081, 0.0724, 0.1059, 0.172, 0.2607, 0.1978, 0.2372, 0.359,
  0.2372, 0.2262, 0.1964, 0.233, 0.2903
- open: 0.0006, 0.0081, 0.0724, 0.1084, 0.1726, 0.2523, 0.19, 0.2398, 0.3245,
  0.2135, 0.1881, 0.1689, 0.1636, 0.1854

**Reading.** Opening the loop does not rescue the run: with acetylcholine forced
to follow the trajectory B5 own network had (so the ratio is back at the tuned
curve by the last third, median level ~1.46 against a reference of 1.457),
accuracy is 1.25-5.60% against B5 17.50-21.45%. So the collapse is not a
feedback loop in which suppression keeps uncertainty high; it is the early
suppression itself -- causal LTP suppressed or inverted through the first third,
while acetylcholine sits near 1.9 -- leaving a deficit the remaining ~10,000
characters never repair. The high acetylcholine of the closed-loop rows is a
consequence of that damage, not its cause.
