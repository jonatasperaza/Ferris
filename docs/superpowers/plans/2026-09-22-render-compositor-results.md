# Render/Compositor Validation Result

Date: 2026-09-22
Hardware: NVIDIA GPU (release build, wgpu backend), Intel CPU

## 60-second run

| t (s) | fps | ms/frame | budget |
|-------|-----|----------|--------|
| 10    | 1611 | 0.62 | OK |
| 20    | 1691 | 0.59 | OK |
| 30    | 1765 | 0.57 | OK |
| 40    | 1544 | 0.65 | OK |
| 50    | 1753 | 0.57 | OK |
| 60    | 1852 | 0.54 | OK |

## Verdict

**PASS**

The average ms/frame across the 60-second run was **0.59 ms/frame**, which is well below the 8.3ms success criterion for the 120fps spec. All six sampled timepoints reported frame times between 0.54ms and 0.65ms, with every reading showing "budget OK" status. The application rendered at approximately 1700 fps on average (uncapped), demonstrating excellent performance headroom for the 120fps target. No frame budget misses were observed throughout the validation period.

**Performance Summary:**
- Minimum frame time observed: 0.54 ms (1852 fps)
- Maximum frame time observed: 0.65 ms (1544 fps)
- Average frame time: 0.59 ms (approximately 1694 fps)
- Budget headroom: 8.3ms - 0.59ms = 7.71ms (13x faster than target)

The render/compositor sub-project successfully demonstrates frame-budget compliance as specified in the design document. The application is ready for sub-project 2.
