# Data: finding 13

Raw data tables for [`findings.md`](../findings.md) finding 13 — Four mechanisms are built, tested and reachable from no caller — found 2026-09-13, same.

---

## Table 1

| cadence                      | confirmation seeds | selection seeds    |
| ---------------------------- | ------------------ | ------------------ |
| no sleep (B5's winner)       | 19.05%             | 20.36%             |
| sleep every 1,500 characters | 19.74% (+0.69)     | 18.95% (−1.41)     |
| sleep every 750 characters   | 19.14% (+0.09)     | 18.67% (−1.69)     |
| sleep every 250 characters   | **13.51% (−5.54)** | **13.41% (−6.95)** |

## Table 2

| condition                    | confirmation       | selection          |
| ---------------------------- | ------------------ | ------------------ |
| no coupling (B5's winner)    | 19.05%             | 20.36%             |
| NA gates predictive learning | 19.28% (+0.23)     | 20.36% (+0.00)     |
| NA gates STDP                | 19.46% (+0.41)     | 20.29% (−0.07)     |
| NA gates both                | 19.46% (+0.41)     | 20.29% (−0.07)     |
| ACh driven, not held at 1.0  | **20.46% (+1.41)** | **19.66% (−0.70)** |
