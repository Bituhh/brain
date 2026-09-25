# Data: finding 17

Raw data tables for [`findings.md`](../findings.md) finding 17 — Growth's
capacity was _unreachable_, not merely unhelpful — closed 2026-09-21 by PLAN.md
C4,.

---

## Table 1

| char index | index-block reach: grown → ORIGINAL | spatial reach r=50: grown → ORIGINAL |
| ---------- | ----------------------------------- | ------------------------------------ |
| 3,000      | 0                                   | 3,253                                |
| 4,500      | 0                                   | 14,180                               |
| 9,000      | 0                                   | 15,011                               |
| 13,500     | 0                                   | **15,822**                           |

## Table 2

| condition                                     | mean   | per seed                          | vs C  | seeds better than C |
| --------------------------------------------- | ------ | --------------------------------- | ----- | ------------------- |
| C: no growth, index-block reach (B5's winner) | 19.05% | 18.90, 18.10, 21.45, 19.30, 17.50 | —     | —                   |
| B: C + growth, burst pace, index blocks       | 19.05% | identical to C on every seed      | +0.00 | 0/5                 |
| E: C + growth, gentle pace, index blocks      | 19.05% | identical to C on every seed      | +0.00 | 0/5                 |
| NG-25: **no growth**, spatial r=25            | 19.50% | 18.95, 20.65, 19.95, 19.25, 18.70 | +0.45 | 3/5                 |
| NG-50: **no growth**, spatial r=50            | 19.86% | 19.15, 20.35, 21.20, 19.70, 18.90 | +0.81 | 4/5                 |
| NG-100: **no growth**, spatial r=100          | 19.62% | 20.75, 19.55, 20.10, 18.70, 19.00 | +0.57 | 3/5                 |
| SB-25: C + growth, burst pace, spatial r=25   | 19.34% | 20.20, 20.15, 19.30, 18.10, 18.95 | +0.29 | 3/5                 |
| SB-50: C + growth, burst pace, spatial r=50   | 19.59% | 18.05, 20.20, 20.70, 19.55, 19.45 | +0.54 | 3/5                 |
| SB-100: C + growth, burst pace, spatial r=100 | 18.78% | 18.80, 19.75, 17.55, 18.55, 19.25 | −0.27 | 2/5                 |
| SE-50: C + growth, gentle pace, spatial r=50  | 19.85% | 19.15, 20.35, 21.20, 19.70, 18.85 | +0.80 | 4/5                 |

## Table 3

| radius           | growth row − its own no-growth control | seeds where growth is worse           |
| ---------------- | -------------------------------------- | ------------------------------------- |
| 25               | −0.16                                  | 3/5                                   |
| 50 (burst pace)  | −0.27                                  | 4/5                                   |
| 50 (gentle pace) | −0.01                                  | 1/5, and **bit-equal on the other 4** |
| 100              | −0.84                                  | 3/5                                   |
