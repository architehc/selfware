# Magic state cultivation

The last lesson ended with an uncomfortable economy: 90–94% of a fault-tolerant machine is factories burning fifteen dirty magic states to purify one. In September 2024, Gidney, Shutty, and Jones published a different way to make a magic state — **cultivation** ([arXiv:2409.17595](https://arxiv.org/abs/2409.17595), "Magic state cultivation: growing T states as cheap as CNOT gates") — and it changes the factory floor entirely. This lesson explains the idea, its three stages, why simulating it needed a brand-new tool, and what the "as cheap as CNOT" headline actually claims.

## Cultivation versus distillation

Distillation is batch processing: gather fifteen dirty states, run them through a checking circuit, throw away the rejects, keep one. Every inefficiency in the last lesson flows from that batch shape — the 15x input multiplier per level, the discarded runs, the whole first level of the factory run at a fixed distance chosen to barely meet the error budget. Distillation also spends its protection bluntly: fifteen inputs, each individually dirty, all wrapped in enough code distance to satisfy the *final* error target.

Cultivation is gardening instead of batch refining. Prepare **one** magic state inside a **small** code, then *grow the code around it*, checking the state's quality at every stage and restarting early — while mistakes are still cheap — whenever a check fails. The state's reliability and the code's distance grow together, so protection is spent only in proportion to quality already earned. No fifteen-fold over-protection of inputs that will mostly be discarded; no fixed-distance factory floor.

The agricultural metaphor is the authors' own framing, and it is accurate about the economics: a distillation factory commits a huge greenhouse up front and throws out most of the harvest; cultivation plants one seed, tends it, and only builds the greenhouse as the plant grows.

The contrast runs deeper than scheduling. Distillation's quality guarantee comes from an error-detecting code applied to a *batch* — the Reed–Muller checks of the last lesson — so its verification is wholesale: pass or fail on fifteen states at once. Cultivation's guarantee is retail: each check interrogates the one state you actually intend to keep. Same underlying physics (postselection on detected errors), radically different inventory management.

## Stage 1: injection into a small color code

The protocol begins, as before, with injection — but wrapped differently. The |T⟩ state is encoded unitarily into a small **color code** at distance 3, and the stabilizers are measured; if anything fired, the attempt is discarded immediately.

Why a color code? It is a cousin of the surface code with one crucial extra symmetry: it supports a transversal Clifford gate under which |T⟩ is a +1 eigenstate. That symmetry is what makes stage 2's direct "is this still a T state?" checks possible — the surface code has no equivalent trick. The seed moment of injection still exists, but it is wrapped in a distance-3 code within a single step, and a retry at distance 3 costs almost nothing. Compare the last lesson: distillation's dirty seed propagated through fifteen inputs before anyone checked anything.

If you have not met color codes: picture the surface code's chessboard recolored so checks come in three colors, with each data qubit touched by more checks than before. The extra structure buys additional transversal gates at the price of weightier, more error-prone check circuits — a trade the surface code refuses for bulk storage, and cultivation exploits for exactly one stage before escaping back to the surface code for the long haul.

## Stage 2: cultivation — check and grow

Now the heart of the idea. Repeat two moves in alternation:

- **Check:** verify "T-ness" directly. Because |T⟩ is a +1 eigenstate of that transversal Clifford, a Hadamard-test-style circuit — an ancilla qubit that interferes the state against its own transformed copy — measures whether the hosted state still has the right eigenvalue. A passed check is affirmative evidence of quality, not merely an absence of detected errors.
- **Grow:** between checks, expand the patch — d = 3 to d = 5 — so the surviving state earns more protection.

Each passed check raises the fault distance of the surviving state; each growth step wraps it in more protection. Trust is *earned in stages*, each stage paid for only after the previous one succeeded. The distance no longer arrives in the discrete, wasteful jumps of a multi-level factory (d = 17 for level 1, d = 34 for level 2, whether the states need it or not); it grows continuously with the state's proven quality.

The cost model is postselection: checks fail sometimes, and a failed check means restarting from an earlier, smaller stage. Restarting a d=3 attempt is nearly free; restarting late is expensive. The whole art of the protocol is arranging the check-and-grow schedule so failures are caught while cheap.

A term to define precisely, since the whole protocol trades on it: **fault distance** is the minimum number of independent physical faults that could conspire to corrupt the state *without* being caught by the checks performed so far. Raw code distance counts faults against an idle memory; fault distance counts them against this specific preparation history. Cultivation's checks are what convert elapsed rounds into earned fault distance — which is why a passed check is worth more than an idle round, and why the schedule (when to check, when to grow) is the protocol's real design variable.

## Stage 3: escape

A cultivated state lives on a small color-code patch, but your computation runs on large surface-code patches at distance ~15 or more. The state must be transferred out — the **escape stage**. The paper's mechanism is **grafting**: a code deformation that morphs the color-code patch directly into a surface-code patch (the "grafted color/surface code"), rapidly growing to full distance.

The paper is admirably honest here: escape is *"surprisingly difficult and costly"* — over 100 qubits — and it dominates the protocol's error budget. It also calls out a bookkeeping sin of the older literature: prior factory papers often ignored the cost of growing a prepared state to full distance, which made factories look cheaper than they were. Cultivation's accounting includes escape, and any honest comparison between protocols must too. When you read a factory-size claim anywhere, the first question to ask is: does this number include getting the state to the distance the algorithm actually needs?

Grafting itself is worth a picture: the small color-code patch is deformed — boundaries moved, checks rewired — until its stabilizer structure continuously becomes that of a surface-code patch, which then grows by ordinary means. It is a code *metamorphosis* performed on live data, and the intermediate grafted code is neither parent: it must be simulated and checked in its own right. This is one reason exact end-to-end simulation (the Clifft story below) mattered — the escape is precisely the stage where hand-waving used to hide.

The whole pipeline, in one view:

```
d=3 color code        d=3 -> d=5 grow+check      d=15 surface code
|T> injected     ->   check, grow, repeat   ->   graft ("escape")
discard if fired      restart early on fail      then ordinary plumbing
```

Each stage is small compared to a distillation factory floor, and each failure is caught at the cheapest possible moment. That is the entire cost argument in one diagram.

## The headline numbers

Simulated under uniform circuit-level depolarizing noise:

- Logical error rate of the cultivated |T⟩: **2 x 10⁻⁹** at physical noise p = 10⁻³ (idling the output at distance 15), and **4 x 10⁻¹¹** at p = 5 x 10⁻⁴ — the protocol responds strongly to better hardware.
- Total cost: about **10x fewer qubit-rounds** than distillation-based factories at comparable error rates — landing near the cost of a single lattice-surgery **CNOT** of equivalent reliability. Hence the title: T states *as cheap as CNOT gates*.
- The catch, stated by the authors: the postselection cost grows exponentially in the *target fault distance*. Cultivation is not an asymptotic construction — it wins in the practical 10⁻⁶–10⁻⁹ regime where real algorithms live, not in the limit.
- The authors go further and conjecture that "further magic state distillation may never be needed in practice."

The end-to-end impact arrived within a year. Gidney's 2025 resource estimate for factoring RSA-2048 ([arXiv:2505.15917](https://arxiv.org/abs/2505.15917)) — under a million noisy qubits, under a week — uses six cultivation-based magic-state factories. The factory, the 90% line item since 2012, shrank to a few grow-and-check patches plus routing.

## The follow-up wave, and the hardware gap

Cultivation became a research program almost immediately. The 2025–2026 literature (verified at title-and-abstract level, no further) includes cultivation directly on the surface code with no color-code detour, fold-transversal surface-code variants, a construction using only two-qubit gates, and escape by lattice surgery instead of grafting. One idea deserves its own sentence: **multiplexed cultivation** runs several injection/cultivation trajectories in parallel inside a single patch and keeps the first survivor, slashing the early-stage discard rate — distillation's buffering problem, attacked with parallelism instead of storage. Which approach ultimately wins — cultivation, code-switching, or classical distillation — is genuinely unsettled; the first direct comparisons only appeared in 2026.

The hardware frontier is further behind than the theory. The nearest experiments are distillation-family, not cultivation: a QuEra/Harvard/MIT team performed logical-level magic state *distillation* on neutral atoms, and Quantinuum reported a code-switched logical T state with infidelity around 5 x 10⁻⁴ (we know this from a secondary summary and have not verified the primary source) — below its best physical two-qubit gate error, a "break-even with magic" milestone. We are not aware of a published experiment demonstrating cultivation itself; treat that absence as unverified. Every cultivation number in this lesson is a Monte Carlo under an assumed noise model, and the hardware lessons of the next-but-one section — leakage, crosstalk, correlated bursts — are not fully modeled in it. Believe the shape of the result; hold the digits loosely.

## Why simulating this needed a new simulator: Clifft

There is a reason cultivation's numbers only appeared in 2024–2026, and it is a simulation wall you already know from both sides. **Stim** handles Clifford circuits at laptop speed but cannot represent a |T⟩ state at all — that is the whole point of Gottesman–Knill. A **statevector simulator** handles anything but costs 2^N amplitudes for N qubits; a cultivation circuit with its escape stage sits far beyond that. Cultivation lived exactly in the gap: too magical for Stim, too big for statevectors.

**Clifft** ([arXiv:2604.27058](https://arxiv.org/abs/2604.27058)) was built for this gap, using **frame factorization**. The idea: track the bulk of the circuit in a Clifford frame — Stim-style, nearly free — and keep a **dynamically sized statevector over only the "virtual" degrees of freedom that are currently non-Clifford-entangled**. That active set *expands* at non-Clifford operations and *contracts* at measurements. The exponential cost therefore scales with the **peak active virtual dimension** — the widest the non-Clifford entanglement ever gets at any moment — not with the total qubit count N.

A cultivation circuit is exactly the regime where this wins: hundreds of qubits, but only a handful of them non-Clifford-entangled at any one time. The payoff, per the authors' benchmarks:

- Within a constant factor of GPU statevector simulators (Qiskit-Aer, Qulacs, qsim) in the dense limit where every qubit is magical — Clifft degrades gracefully instead of refusing to run.
- Within about an order of magnitude of Stim in the pure-Clifford limit, where the active set stays empty.
- In the target low-magic regime: **up to ~370x faster than Tsim** (a GPU near-Clifford sampler) on the d=3 cultivation circuit — on commodity CPUs, against a GPU baseline.

Clifft also compiles circuits (d=5 cultivation) that the earlier samplers could not finish within budget, and it enabled the first exact end-to-end cultivation simulation *including the escape stage*. Treat the 370x as the authors' claim on their benchmark — but note the qualitative fact that needs no benchmarking: before frame factorization, an exact end-to-end number for cultivation with escape simply did not exist. You will drive Clifft yourself in the tier 5 hands-on — the d=3 cultivation circuit above is your final project.

Two engineering details complete the picture. Clifft compiles ahead of time and executes on what its authors call a "Schrodinger Virtual Machine": the Clifford frame, an online Pauli frame, and the active statevector advance together, so shots after the first are nearly free — Stim's compile-once/sample-many model, generalized to circuits with T gates. And the paper ships with reproducibility artifacts, so the 370x figure is checkable rather than merely quoted.

## What changes for the compiler

The tier 3 pipeline does not disappear under cultivation — but its optimization target moves. Under distillation, the compiler's hard problem was *place and schedule factories*: site the pyramid, route the outputs, buffer the rejects. Under cultivation, the factory is one gradually grown patch, so the problem becomes *schedule distance growth and escape*: when to check, when to grow, when to graft, and where the >100-qubit escape real estate lives on the floor plan.

The cost model survives intact: space-time volume in qubit-rounds is still the currency, and the tier 3 rule — a design is not believed until its compiled circuit has been Monte-Carlo'd below threshold — now applies to cultivation circuits specifically. That is precisely the study Clifft was built to run, and the one you will run in tier 5.

## What "as cheap as CNOT" does and does not claim

Precision matters, because this headline is easy to over-read. The claim is: the **total spacetime cost** of producing one cultivated |T⟩ at a useful error rate — injection, cultivation, postselection retries, and the escape, all counted — is comparable to one lattice-surgery CNOT at equivalent reliability. It is a qubit-rounds statement, honest because it includes the escape, and impressive because distillation's equivalent number is ~10x larger.

It does **not** claim that magic states are free, that factories disappear (you still need cultivation patches, escape real estate, and routing), or that the 10x holds at every noise rate and target distance — the exponential-in-fault-distance postselection caveat guarantees it eventually breaks. And it is not yet a hardware result. The correct mental model: the factory's share of the machine shrank from ~90% toward the size of an ordinary logic block — *if* the simulations survive contact with real noise.

## Open problems, as the field sees them

The frontier notes are unusually candid about what is not yet done:

- **Escape is the bottleneck.** Injection and cultivation are cheap; grafting to a distance-15 surface code dominates both cost and error budget. Better graft constructions and escape-via-lattice-surgery are active problems.
- **Postselection waste.** Early-stage discard rates drive real spacetime cost; multiplexing helps, but how far parallelism scales before correlated failures dominate is open.
- **The horse race is unsettled.** Cultivation vs code-switching vs distillation has no declared winner — the answer depends on physical noise, connectivity, and target error rate. Best practice when comparing: quote qubit-rounds *including* growth and escape to final distance, at a stated noise model.
- **Beyond |T⟩.** Square-root-T states, other codes, other gates — the grow-and-check idea is generalizing faster than anyone can benchmark it.

## Key numbers

- Cultivation = prepare one |T⟩ inside a small code and **grow distance during preparation**, checking and restarting early; distillation = batch-refine 15 dirty states into 1 clean one at fixed distances.
- Three stages: **injection** (d=3 color code), **cultivation** (check-and-grow cycles, d=3 to d=5), **escape** (grafting into a large surface code — "surprisingly difficult and costly", >100 qubits, dominates the error budget).
- Output quality: **2 x 10⁻⁹** logical error at p = 10⁻³ (4 x 10⁻¹¹ at p = 5 x 10⁻⁴), idling at d=15; **~10x fewer qubit-rounds** than distillation — "as cheap as CNOT".
- Caveat: postselection cost grows exponentially in target fault distance; all numbers are simulation-only so far. RSA-2048 under a million qubits (arXiv:2505.15917) uses six cultivation factories.
- **Clifft** (frame factorization; cost scales with peak active virtual dimension, not qubit count) made exact end-to-end cultivation simulation possible — **~370x Tsim** on d=3 cultivation (authors' benchmark claim).

## Next

A cheap magic state is only half the supply problem. Consuming it by teleportation means a measurement outcome must be decoded and acted on *while the quantum computer waits* — every microsecond of decoder lag is a microsecond off the logical clock. That is the latency race: [real-time decoding](#/lesson/realtime-decoding). Then, in tier 5, the cultivation circuit from this lesson becomes your own simulation: [the Clifft hands-on](#/lesson/hands-on-clifft) reproduces the d=3 grow-and-check sequence whose cost you just read about.
