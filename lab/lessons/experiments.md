# Experiments: threshold to below-threshold

Every number in the earlier tiers came from theory or simulation. This closing lesson of tier 4 is about hardware: three experiments, a decade apart, that took the surface code from "components good enough in principle" to "a logical qubit that outlives its parts." Read it as a single story with three chapters — and as a calibration exercise, because the gaps between these results and the theory are as instructive as the results.

## 2014: components at the threshold (but no logical qubit)

The opening chapter is Barends et al. ([arXiv:1402.4848](https://arxiv.org/abs/1402.4848)): five **Xmon** transmon qubits in a line — superconducting circuits, Google's qubit of choice ever since. There is no code here, no logical qubit, not even a syndrome extraction circuit. What the paper demonstrated is that *individual operations* had crossed the surface code's ~1% error threshold:

- Single-qubit gates: **99.92% average fidelity** across all gates and qubits — measured by randomized benchmarking with all gates running *simultaneously*, the honest way, so crosstalk is included rather than assumed away.
- Two-qubit CZ gates: **up to 99.44 ± 0.05%**, and 99.0–99.44% across all four nearest-neighbor pairs.
- Coherence times of tens of microseconds: enough room for many gate operations before decoherence — though even then, parasitic two-level defects punched dips below 10 µs at some frequencies, an early sighting of the drift problem that returns below.
- Entangling power checked directly: Bell-pair fidelity 99.5%, five-qubit GHZ state 81.7%. The CZ error budget was dominated by decoherence (55%), with control errors (24%) and **leakage** (21%) — remember leakage; it becomes a recurring villain.

The claim was deliberately modest and historic: for the first time, the *ingredients* of a surface code met the code's entrance requirement. But note what was missing — readout at speed was *assumed*, not demonstrated, in the paper's threshold analysis, and a 1-D line of five qubits cannot even run one distance-3 patch. Think of it as a foundry proving it can cast every part of an engine to tolerance, years before any engine turns over.

One more detail reveals how aspirational the threshold claim was. The paper's surface-code simulation appendix assumed measurement at 99% fidelity in 200 ns and reset at 99% in 50 ns — numbers the device had not yet demonstrated at speed. The gates were real; the full syndrome-extraction cycle was still a simulation. That gap between "components at threshold" and "a cycle at threshold" took the field most of the following decade to close.

## 2022: the d=3 to d=5 crossover — barely

Eight years later, Google Quantum AI ran the real experiment on a 72-qubit Sycamore device ([arXiv:2207.06431](https://arxiv.org/abs/2207.06431)): distance-3 and distance-5 surface codes, side by side, 25-cycle memories, 921 ns cycles (500 ns of that is measurement, 160 ns reset — the cycle is mostly spent *looking*, a fact the real-time decoding lesson exploits). Component errors were measured per-qubit and per-pair, not averaged. The question was the one tier 1 promised: does making the code *bigger* actually make the logical qubit *better*?

The answer was a qualified, fragile yes:

- Logical error per cycle: ε₃ = (3.028 ± 0.023)% at d=3; ε₅ = (2.914 ± 0.016)% at d=5.
- The suppression factor **Λ = ε₃/ε₅ ≈ 1.04** — barely above 1. Recall from tier 1 that Λ is the error reduction bought per two units of distance; Λ > 1 is the operational definition of "below threshold." At 1.04, the device was *at* threshold, more than below it. Two of the four individual d=3 patches actually beat the d=5.
- The result was decoder-fragile: a faster but sloppier decoder (belief-matching) gave ε₅ = 3.056% and *erased* the d=3→d=5 gain entirely — early evidence of the speed/accuracy trade from the last lesson.
- The component error table explained the fragility. The worst actors were not the gates: data qubits *idling* during measurement and reset errored at 2.5 x 10⁻², and readout at 2.0 x 10⁻², against CZ gates at 6 x 10⁻³ and single-qubit gates at 1.1 x 10⁻³. The error budget is not where a gate-centric intuition looks for it.
- **Leakage** — qubits escaping the two-level computational subspace — had an outsized effect: per unit of error probability it damaged Λ more than twice as much as an ordinary CZ error, it spread spatially between gate partners, and without active removal it *accumulated*, causing the d=5 code to degrade faster than d=3 over long runs.
- A sobering floor: a distance-25 repetition-code probe bottomed out at a logical error rate of ~1.7 x 10⁻⁶ per round, set by a single **correlated burst** — a cosmic-ray-like impact event that hammered the whole chip at once (excluding that one event, the floor dropped to ~1.6 x 10⁻⁷). Independent-error theory does not predict such events; hardware does not care.

The takeaway: scaling worked, in aggregate, by 4%. The era of "more qubits reliably help" had not yet begun.

Two technical choices in that experiment became field standards. The code was a ZXXZ variant with dynamical decoupling applied to idling data qubits — a direct attack on the idle-error line item. And the decoder was calibrated from the device itself: matching-graph edge weights built from measured pairwise detection-event correlations rather than an assumed noise model, with odd-numbered trials' data used to decode even-numbered trials, so the decoder never trains on the data it is graded on.

The same data delivered a warning about error models. A pure-Pauli simulation systematically *underpredicted* the observed correlations between detection events; only a "Pauli+" model — Pauli noise plus leakage, leakage transport between gate partners, crosstalk, and stray interactions — reproduced them. A decoder or simulator fed independent-Pauli fiction mispredicts both the error rate and its structure. This is why serious resource estimates demand calibrated, per-component noise tables.

## 2024: below threshold, beyond break-even

Two years later, the Willow-generation work ([arXiv:2408.13687](https://arxiv.org/abs/2408.13687)) — a 105-qubit processor for the d=7 code and a 72-qubit processor for d=5 with the real-time decoder; mean coherence ~68 µs, **1.1 µs** cycles, per-cycle leakage removal, and gap-engineered junctions against impact events — turned the 4% into a factor of two. Component errors improved roughly 2–3x across the board (CZ gates to 2.8 x 10⁻³, readout to 8 x 10⁻³), and the leakage removal alone bought a 35% improvement in Λ:

- **Λ = 2.14 ± 0.02**: each step of +2 in distance now *halves* the logical error rate. Measured on d=3, d=5, and d=7 codes simultaneously, the exponential suppression from tier 1 became a hardware fact.
- **ε₇ = 0.143% ± 0.003%** logical error per cycle at distance 7 (0.30% at d=5, 0.65% at d=3 — the Λ ladder, visible on one device).
- **Beyond break-even**: the d=7 logical qubit lived **291 ± 6 µs**, versus 119 ± 13 µs for the *best* constituent physical qubit (median 85 µs) — a lifetime **2.4 ± 0.3x longer than any of its parts**. This is the field's baseline acceptance test, and 2024 was the first time a superconducting logical memory passed it: the redundancy is now buying life, not spending it.
- **Endurance**: the real-time decoder from the last lesson sustained below-threshold operation for **10⁶ cycles** (~1.1 seconds), and repetition codes up to d=29 showed suppression factor Λ = 8.4 ± 0.1, tracking exponential improvement down to an apparent logical error floor of **~10⁻¹⁰ per cycle** over 2 x 10¹⁰ total cycles.

Put the three eras on one line: 2014 proved the parts; 2022 proved scaling *can* help (Λ ≈ 1.04, barely); 2024 proved scaling *does* help, decisively (Λ = 2.14), and that a logical qubit can outlive its hardware. The exponential promise of QEC is no longer a plot from a simulator.

The error budget shifted accordingly: CZ gate errors now dominate (41% of the 1/Λ budget), with data-qubit idle (20%), readout (11%), and CZ crosstalk (11%) behind — the correlated errors from two-qubit gates are explicitly named the largest addressable target. A measurement subtlety became visible too: a popular rule of thumb equates 1/Λ with the weight-4 detection probability, but with leakage removal switched off, Λ moved 35% while the detection probability moved only 12%. Under correlated noise, the cheap proxy and the actual performance decouple — yet another reason to measure, not assume.

## The Λ lever, and how honest people use it

Both Google papers organize their physics around one formula. Below threshold, the logical error per cycle scales as ε_d ~ (p / p_threshold)^((d+1)/2), so the ratio

```
Lambda = epsilon_d / epsilon_{d+2}  ≈  p_threshold / p
```

is approximately *inversely proportional to the physical error rate*. Consequences worth memorizing:

- **Halving component errors doubles Λ.** The leap from Λ ≈ 1.04 (2022) to Λ = 2.14 (2024) came from a mere 2–3x improvement in component errors. The exponential leverage is real — and it cuts both ways for anyone projecting forward.
- **Λ has been measured only at d ≤ 7** (d ≤ 29 for repetition codes, which correct only one error type). Every factory-distance projection (d = 15–31) is an extrapolation, carrying the ±0.02 measurement uncertainty and a ~20% gap between the best noise model and the device.
- The error budget *linearizes* in 1/Λ: each component contributes its error rate times a sensitivity weight — and the weights differ by more than an order of magnitude (in 2022, leakage's weight was 125 versus readout's 5.6). Equal component errors do not contribute equally; fixing the wrong one buys nothing.

The 2024 paper spells out the planning consequence: each factor-of-2 reduction of physical error multiplies Λ by ~2, so the qubit count for a target algorithm drops exponentially with hardware improvement. That is the honest version of "hardware progress compounds" — and the reason resource estimates must be re-run against each new device generation, not rescaled from the last one.

## The new frontier: the correlated-burst floor

The 10⁻¹⁰ floor deserves its own section, because it redefines what "hard" means. Over the 2 x 10¹⁰ repetition-code cycles, the 2024 device saw six large error bursts — about one per hour — qualitatively different from the 2022 cosmic-ray impacts: spatially localized, anisotropic, decaying over ~370 µs, cause unknown. These bursts set the apparent 10⁻¹⁰-per-cycle floor.

Why this matters more than any gate-error number: every decoder model, every simulation, every resource estimate in this lab assumes *independent-ish circuit-level noise*. A burst is a many-detection, temporally extended, non-Pauli event that no such model generates — and it dominates precisely in the regime (logical error below ~10⁻⁹ per cycle) where fault-tolerant algorithms live. You cannot distance your way past it: the exponential suppression that took the field from 3% to 0.143% flattens at the burst floor. Mitigation will look like physics and engineering — gap engineering, shielding, erasure flagging, spatially separated patches — not like bigger codes. When a resource estimate quotes 10⁻¹³, ask what it assumed about bursts.

Also file away the **drift** problem: component performance is non-stationary. Coherence times wander on hour timescales as two-level defects drift through the frequency spectrum (the 2024 work goes so far as to *forecast* defect trajectories when choosing operating frequencies), and the 15-hour measurement runs recalibrated every few hours. A noise model is a snapshot, not a law. Over those 15 hours and 16 runs, Λ itself averaged 2.18 ± 0.07, with a best run of 2.31 — the machine's suppression factor is a random variable, not a constant. Encouragingly, the data showed larger codes are empirically more robust to this drift: fluctuations that visibly hit individual d=3 patches were filtered out at d=5.

## Error models: what held and what broke

A decade of device data amounts to a stress test of the error models the whole field simulates against. What held:

- Circuit-level "Pauli+" models reproduce the detection-event correlation structure well — the best 2024 model overpredicted Λ by only 20%, so most (not all) of the physics is captured.
- Pauli twirling — the assumption that coherent errors can be treated as stochastic — showed no detectable violation at 2022 error rates.
- The ε_d ~ (p / p_threshold)^((d+1)/2) phenomenology fit d = 3, 5, 7 with a single Λ.

What broke:

- Pure independent-Pauli models miss the correlations — leakage and stray interactions are mandatory terms, not refinements.
- Stationarity: drift makes any single calibration snapshot stale within hours.
- The rare-event tails, decisively: the 10⁻⁶ and 10⁻¹⁰ burst floors are entirely outside circuit-level models. No sampled Pauli channel generates them, and a million-shot Monte Carlo will never see a once-an-hour event.

## What this means for everything upstream

The three lessons before this one all end in claims that must survive this lesson's physics:

- **Factories and cultivation** are simulated under noise models; the experiments say calibrated, per-component, per-location noise — with leakage and crosstalk as first-class channels — or your factory estimate is calibrated to fiction. Uniform depolarizing noise misranks the budget: in 2022, idling and readout dominated over gate errors.
- **Real-time decoding** has a measured accuracy price (Λ 2.0 real-time vs 2.18 offline); resource estimates must state which decoder operating point they assume, and compilers must reserve the margin.
- **Break-even is the acceptance test.** A logical memory that does not outlive its best physical qubit is overhead, not progress; simulators should report that ratio, not just a logical error rate.
- **Drift belongs in the schedule.** If coherence times wander hourly, a compiler that fixes qubit assignments and calibrations statically inherits the collision risk; plan for periodic recalibration, as the 2024 runs did over their 15-hour campaigns.

This is the honest state of the art: exponential suppression is real, break-even is beaten, and the remaining obstacles are correlated, bursty, and unglamorous.

## How to read the next below-threshold claim

The three-paper arc doubles as a checklist for evaluating whatever hardware result lands next:

- Are component errors measured *simultaneously* and per-location, or in flattering isolation?
- Is Λ reported with uncertainties, across at least three distances — and which decoder produced it, at what latency, offline or in the loop?
- Does the logical qubit beat its *best* physical constituent, the break-even test?
- Were rare events hunted — long runs, burst analysis — or averaged away in short campaigns?
- Does the noise model reproduce the device's correlations, or just its averages?

The 2024 paper answers all five; the 2014 paper, read strictly, answers only the first. Knowing which questions a result leaves open is most of what "understanding an experiment" means in this field.

## Key numbers

- **2014** (arXiv:1402.4848, 5 Xmons): 1-qubit gates **99.92%**, CZ **up to 99.44 ± 0.05%** — components at the ~1% threshold; **not a logical qubit**, no code run.
- **2022** (arXiv:2207.06431, 72 qubits): d=3→d=5 with **Λ ≈ 1.04** — crossover regime, scaling barely helps; fast decoding erased the gain; idle and readout errors topped the budget; a correlated burst set a ~10⁻⁶ repetition-code floor.
- **2024** (arXiv:2408.13687, 105 qubits): **Λ = 2.14 ± 0.02** below threshold; **ε₇ = 0.143% ± 0.003%** per cycle at d=7; logical lifetime **2.4 ± 0.3x** the best physical qubit (break-even beaten); real-time decoding for **10⁶ cycles**; repetition codes to d=29 (Λ = 8.4 ± 0.1) down to a **~10⁻¹⁰ correlated-burst floor** — the new frontier.
- **Λ ≈ p_threshold / p**: halving physical error doubles the suppression per +2 distance — measured only at d ≤ 7, so factory-distance projections are extrapolations.

## Next

You have reached the end of the reading track: the code, the decoding, the plumbing, the factories, the latency race, and the hardware evidence. Tier 5 is different in kind — no more prose about tools, just the tools. Set up your environment, then draw and compile a blockgraph with tqec, compile a circuit with TopoLS, and simulate cultivation with Clifft. The on-ramp: [hands-on 0: environment setup](#/lesson/setup-env).
