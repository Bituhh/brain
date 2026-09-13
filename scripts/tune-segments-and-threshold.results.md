# segmentsPerNeuron x targetRate joint search log (Phase 7 VAL-4 resurfaced, Phase B)

Generated 2026-09-13T14:51:21.290Z by scripts/tune-segments-and-threshold.ts.

Every trial uses the official 5-seed protocol (seeds [1,2,3,4,5], 15,000-character corpus slice) -- see this file's own module doc for why search-time and confirm-time seed counts are not separated here.

| segmentsPerNeuron | targetRate | seeds | mean network accuracy | range across seeds | mean trigram accuracy | note |
|---|---|---|---|---|---|---|
| 1 | 0.5000 | 5 | 8.05% | 6.95%-9.40% | 28.40% | search |
| 1 | 0.5500 | 5 | 7.85% | 6.55%-8.95% | 28.40% | search |
| 1 | 0.4500 | 5 | 7.99% | 5.30%-10.55% | 28.40% | search |
| 1 | 0.5250 | 5 | 8.71% | 7.20%-10.35% | 28.40% | search |
| 1 | 0.4750 | 5 | 8.28% | 6.45%-9.45% | 28.40% | search |
| 1 | 0.5375 | 5 | 8.94% | 7.35%-9.40% | 28.40% | search |
| 1 | 0.5125 | 5 | 8.46% | 8.25%-8.70% | 28.40% | search |
| 1 | 0.5437 | 5 | 8.05% | 7.55%-8.40% | 28.40% | search |
| 1 | 0.5313 | 5 | 8.39% | 7.00%-9.05% | 28.40% | search |
| 1 | 0.5406 | 5 | 8.47% | 7.65%-9.15% | 28.40% | search |
| 1 | 0.5344 | 5 | 8.37% | 7.45%-9.30% | 28.40% | search |
| 1 | 0.5375 | 5 | 8.94% | — | — | **best for this segmentsPerNeuron (already logged above; restated for the summary table)** |
| 2 | 0.5000 | 5 | 6.47% | 5.55%-7.00% | 28.40% | search |
| 2 | 0.5500 | 5 | 6.66% | 5.95%-7.75% | 28.40% | search |
| 2 | 0.4500 | 5 | 5.72% | 4.85%-6.70% | 28.40% | search |
| 2 | 0.6000 | 5 | 7.03% | 5.80%-7.55% | 28.40% | search |
| 2 | 0.6500 | 5 | 7.94% | 6.00%-9.05% | 28.40% | search |
| 2 | 0.7000 | 5 | 8.11% | 6.45%-8.90% | 28.40% | search |
| 2 | 0.7500 | 5 | 8.42% | 5.90%-9.85% | 28.40% | search |
| 2 | 0.8000 | 5 | 9.61% | 7.65%-10.70% | 28.40% | search |
| 2 | 0.8500 | 5 | 10.37% | 8.55%-11.85% | 28.40% | search |
| 2 | 0.9000 | 5 | 12.77% | 10.75%-14.65% | 28.40% | search |
| 2 | 0.9500 | 5 | 17.13% | 16.05%-18.35% | 28.40% | search |
| 2 | 0.9900 | 5 | 17.37% | 15.75%-18.55% | 28.40% | search |
| 2 | 0.9400 | 5 | 16.94% | 16.00%-18.35% | 28.40% | search |
| 2 | 0.9650 | 5 | 17.19% | 16.05%-18.25% | 28.40% | search |
| 2 | 0.9775 | 5 | 17.28% | 16.50%-18.45% | 28.40% | search |
| 2 | 0.9838 | 5 | 17.27% | 16.15%-18.40% | 28.40% | search |
| 2 | 0.9869 | 5 | 16.90% | 15.50%-18.25% | 28.40% | search |
| 2 | 0.9900 | 5 | 17.37% | — | — | **best for this segmentsPerNeuron (already logged above; restated for the summary table)** |
| 3 | 0.5000 | 5 | 4.43% | 3.20%-6.60% | 28.40% | search |
| 3 | 0.5500 | 5 | 4.18% | 3.30%-5.60% | 28.40% | search |
| 3 | 0.4500 | 5 | 3.76% | 2.85%-4.90% | 28.40% | search |
| 3 | 0.5250 | 5 | 4.28% | 2.85%-6.55% | 28.40% | search |
| 3 | 0.4750 | 5 | 4.27% | 3.10%-5.50% | 28.40% | search |
| 3 | 0.5125 | 5 | 4.63% | 3.50%-6.50% | 28.40% | search |
| 3 | 0.4875 | 5 | 4.17% | 3.10%-4.95% | 28.40% | search |
| 3 | 0.5188 | 5 | 4.36% | 3.50%-6.75% | 28.40% | search |
| 3 | 0.5062 | 5 | 3.76% | 3.00%-4.95% | 28.40% | search |
| 3 | 0.5156 | 5 | 4.17% | 2.95%-5.45% | 28.40% | search |
| 3 | 0.5094 | 5 | 3.92% | 2.50%-5.40% | 28.40% | search |
| 3 | 0.5125 | 5 | 4.63% | — | — | **best for this segmentsPerNeuron (already logged above; restated for the summary table)** |
| 4 | 0.5000 | 5 | 5.30% | 4.35%-6.65% | 28.40% | search |
| 4 | 0.5500 | 5 | 5.39% | 3.30%-7.00% | 28.40% | search |
| 4 | 0.4500 | 5 | 5.01% | 4.45%-5.75% | 28.40% | search |
| 4 | 0.6000 | 5 | 7.37% | 4.60%-9.95% | 28.40% | search |
| 4 | 0.6500 | 5 | 8.68% | 7.05%-10.20% | 28.40% | search |
| 4 | 0.7000 | 5 | 8.82% | 7.50%-9.65% | 28.40% | search |
