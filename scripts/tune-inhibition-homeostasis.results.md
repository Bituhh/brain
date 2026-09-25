# Inhibition-homeostasis targetRate search log

Generated 2026-09-13T09:59:24.696Z by scripts/tune-inhibition-homeostasis.ts.

| targetRate | seeds | mean network accuracy | range across seeds | note                                        |
| ---------- | ----- | --------------------- | ------------------ | ------------------------------------------- |
| disabled   | 3     | 17.60%                | 15.75%–18.55%      | baseline (mechanism disabled)               |
| 0.0400     | 3     | 1.98%                 | 1.90%–2.10%        | search                                      |
| 0.0600     | 3     | 14.57%                | 14.10%–15.00%      | search                                      |
| 0.0800     | 3     | 17.60%                | 15.75%–18.55%      | search                                      |
| 0.1200     | 3     | 16.82%                | 16.60%–17.10%      | search                                      |
| 0.1600     | 3     | 16.65%                | 16.65%–16.65%      | search                                      |
| disabled   | 5     | 17.37%                | —                  | **confirmed (official protocol), baseline** |
