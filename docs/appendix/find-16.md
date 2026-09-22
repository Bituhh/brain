# Data: finding 16

Raw data tables for [`findings.md`](../findings.md)
finding 16 — Dopamine carried a *raw reward*, not a reward prediction error — closed 2026-09-20 by PLAN.md.

---

| condition | confirmation seeds | selection seeds |
|---|---|---|
| no reward signal (B5's winner) | 19.05% | 20.36% |
| raw reward (dopamine as C3 found it) | 18.53% (−0.52) | 19.82% (−0.54) |
| RPE, `tauEvents` 50 | 19.00% (−0.05) | 20.36% (+0.00) |
| RPE, `tauEvents` 200 | 19.00% (−0.05) | 20.36% (+0.00) |
| RPE, `tauEvents` 1000 | 19.00% (−0.05) | 20.36% (+0.00) |
| baseline configured, nothing rewards (control) | 19.05% | 20.36% |
