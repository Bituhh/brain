# Data: finding 9

Raw data tables for [`findings.md`](../findings.md)
finding 9 — Self-tuning k-WTA sparsity (inhibition-homeostasis spec, Requirement 1): no effect at.

---

| targetRate                                          | smoothing | adjustmentRate | minK | intervalTicks | seeds | mean network accuracy | range across seeds |
| -----------------------------------------------------| -----------| ----------------| ------| ---------------| -------| -----------------------| --------------------|
| *(mechanism disabled — item 8's 17.37% baseline)*    | —         | —              | —    | —             | 5     | 17.37%                | —                   |
| 0.08 (== `NETWORK_DENSITY`, today's fixed ratio)     | 0.9       | 4.0            | 1    | 200           | 3     | 17.60%                | 15.75–18.55%        |
| 0.04                                                 | 0.9       | 4.0            | 1    | 200           | 3     | 1.98%                 | 1.90–2.10%          |
| 0.06                                                 | 0.9       | 4.0            | 1    | 200           | 3     | 14.57%                | 14.10–15.00%        |
| 0.12                                                 | 0.9       | 4.0            | 1    | 200           | 3     | 16.82%                | 16.60–17.10%        |
| 0.16                                                 | 0.9       | 4.0            | 1    | 200           | 3     | 16.65%                | 16.65–16.65%        |
