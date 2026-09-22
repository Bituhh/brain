# Data: decision 13

Raw data tables for [`decisions.md`](../decisions.md)
decision 13 — A dendritic vote is weighted by the synapse's weight, capped at one full vote — and with.

---

## Table 1

| condition, confirmation seeds 11–15 | mean |
|---|---|
| **the winner (weighted votes, sprouting on)** | **19.05%** |
| condition A, no structural plasticity, count mode | 16.99% |
| the winner's exact config with sprouting disabled | 15.58% — worse on all 5 seeds |
| condition A at the winner's own vote settings | 15.58% — identical per seed to the row above |
| B4's count-mode winner (decision 12) | 15.58% |
| runner-up (count mode at the winner's other values) | 8.13% |
| "always guess space", zero learning, zero context | 16.56% (docs/findings.md finding 7) |

## Table 2

| vote mode | silent gate | learning target | mean |
|---|---|---|---|
| weighted | off | permanence | **19.05%** |
| weighted | off | both / weight | 16.24% / 15.54% |
| weighted | on | weight / both / permanence | 14.62% / 14.62% / 10.89% |
| count | on | both / permanence / weight | 10.03% / 6.88% / 5.57% |
| count | off | permanence / both | 8.13% / 8.13% |
| count | off | weight | 0.10% — decision 11's inert case, reproduced exactly |

## Table 3

| condition | mean | per seed vs condition C |
|---|---|---|
| C: structural plasticity, no growth | 19.05% | — |
| B: C + growth, burst pace | 19.05% | identical on all five seeds |
| E: C + growth, gentle pace | 19.05% | identical on all five seeds |
| F: C + growth, gentle pace, sprout-source-restricted | 19.12% | one seed better, one worse |
| D: C + growth, burst pace, sprout-source-restricted | 20.05% | four better, one tied |
