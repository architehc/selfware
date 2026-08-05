# Real-time decoding and the latency race

Everything you decoded in tier 2 was leisurely: collect the syndrome data, run matching, take as long as you like. That is fine for a memory experiment — store a qubit, decode afterward, publish the error rate. A *computer* cannot work that way. This lesson is about why decoding must happen while the machine runs, what happens if the decoder loses the race against the hardware's clock, and how Google closed that gap on a real device in 2024.

## Post-processing versus real-time

Two ways to run a decoder:

- **Offline (post-processing).** Run the experiment, record every syndrome, decode afterward on a classical machine with no deadline. Every decoding result in tiers 1–2 — and most published logical error rates, including Google's 2022 d=3/d=5 paper ([arXiv:2207.06431](https://arxiv.org/abs/2207.06431)) — are offline numbers. Offline decoders can be arbitrarily heavy: neural networks, tensor networks, anything that buys accuracy with time. (Tensor-network decoding is nearly optimal but its cost grows exponentially in d² — a laboratory instrument, not a control system.)
- **Real-time (streaming).** Decode as the syndromes arrive, round by round, keeping pace with a hardware cycle of about **1 µs**. The decoder becomes part of the machine's control loop, not an analysis step.

Why does the distinction matter at all? Because of the Pauli frame discipline from earlier tiers. All *Clifford* corrections can be tracked in classical software and commuted to the end of the circuit — so a pure Clifford computation (like a memory experiment) genuinely tolerates offline decoding. Best practice is to never apply Pauli corrections physically at all: track the frame, fold it into how measurement outcomes are interpreted. The trouble starts with the non-Clifford gates from the last two lessons.

## T-gate teleportation forces feed-forward

Recall how a magic state is consumed: entangle it with the data, measure, and — half the time — apply an S correction based on the outcome. That outcome is a *logical* measurement result, and its meaning depends on the decoder's current estimate of the error state of the machine. You cannot track it in the Pauli frame, because the correction is not a Pauli-by-Clifford bookkeeping entry: the *next quantum operation to perform* depends on it.

This is **feed-forward**, and it turns decoder latency into clock speed. The chain is:

```
syndrome out -> decode -> interpret logical outcome -> decide next gate -> apply
```

The total trip is the **reaction time**, and every dependent non-Clifford gate in the algorithm waits for it. Resource estimates conventionally assume a ~10 µs reaction time; tier 3's time-optimal picture (runtime = T-depth x one measurement time) quietly assumed that classical leg is fast. If the decoder lags, every T layer stalls, pending decisions pile up behind it, and the logical clock slows — potentially catastrophically, once the backlog mechanism below kicks in. The decoder is not an accessory to the quantum computer; it *is* the computer's metronome.

You have seen this dependency before, in disguise. Tier 3's delayed-choice plumbing — chimney pipes with undecided caps, AutoCCZ blocks — is the diagram-level version of the same idea: leave decisions open until the classical system can make them, then commit instantly. Every open pipe cap is a synchronization point where the quantum program pauses for a classical answer. The more aggressively a compiler uses time-optimal tricks, the more of these points it creates, and the more the machine's speed becomes the decoder's speed. Fast classical feedback is not an alternative to clever plumbing; it is what clever plumbing purchases.

## Three metrics, not one

"Fast decoder" conflates three separate requirements, and a real system must hit all three:

- **Accuracy**: the per-cycle probability that the decoded logical outcome is correct. This is what tiers 1–2 optimized.
- **Latency**: the time from syndrome data arriving to the decoded answer being available — including input transport, the software itself, and output delivery.
- **Throughput**: syndrome processed per unit time, which must meet or exceed syndrome *generated* per unit time, sustained indefinitely.

An offline paper reports the first. A real-time system is judged on all three — and the third is a hard gate, not a soft target, because of what happens when you miss it.

## The latency cliff and the backlog problem

A superconducting surface-code cycle takes ~1 µs (1.1 µs on Google's 2024 chip) and produces a fresh batch of syndrome data every cycle. That sets the throughput bar concretely: the decoder must process one round per microsecond per patch, *forever*.

Suppose throughput falls even slightly below that bar. Undecoded syndrome accumulates. But decoding cost grows with the volume of pending data, so the next round's job is bigger and slower — which falls further behind. The backlog does not grow linearly; it **snowballs** (the runaway is exponential in the classic analysis), and once the decoder's estimate is stale enough, every feed-forward decision downstream is being made on wrong information. This is the **backlog problem**: a decoder slower than decoherence is not a slow decoder, it is a crashed quantum computer. The cliff is binary — above the bar you can run indefinitely; below it, you are finished.

A subtle corollary: "fast on average" is not sufficient. Bursty hard instances will occasionally slow any decoder, so a real-time system needs headroom and buffering to absorb spikes and *catch up*. How Google's decoder implements exactly that is the next section's subject.

## Streaming architecture: blocks, fusion, and graceful degradation

The 2024 decoder is worth a look under the hood, because its architecture is the tier 2 streaming ideas made concrete. Detection events stream from the control electronics into a **constant-size graph buffer** — a sliding window over spacetime, independent of total run length, which is why memory does not grow with the experiment. The window is tiled into 10-cycle blocks, one thread per block; detections near a block's edge may match outward, and unresolved boundaries are **fused** downstream into a global matching. If a configuration arrives that no future fusion step can resolve, the system *heralds* a failure rather than silently mis-decoding — an honest error bar instead of a hidden mistake.

Two slack mechanisms keep the average case from becoming the worst case. Processing is provisioned with roughly 2x speed headroom over the quantum data rate, so exponentially rare hard matchings cause linear, recoverable delays rather than fatal ones. And blocks stall *locally*: a hard instance in one spacetime region never blocks threads working elsewhere. The design goal is not "never slow down" — it is "slow down gracefully, catch up, and never let the queue grow without bound." The constant-latency-to-10⁶-cycles result below is the proof that this works.

## Google's integrated decoder: the 2024 numbers

The 2024 below-threshold paper ([arXiv:2408.13687](https://arxiv.org/abs/2408.13687) — the Willow paper, *not* the 2022 scaling paper, which was offline-only) shipped a decoder inside the control loop:

- **63 ± 17 µs average latency** at distance 5 — measured against a **1.1 µs cycle time**.
- **Throughput below one cycle per round, sustained**, with latency **constant up to 10⁶ consecutive cycles** (~1.1 seconds of continuous operation). Constant latency at 10⁶ rounds is precisely the empirical statement "no backlog": the queue never grows.
- Architecture: a parallelized, correlation-augmented **Sparse Blossom** decoder (the tier 2 matching algorithm, in C++, with X/Z correlations folded into the edge weights before matching) running on CPUs, fed detection events streamed from the control electronics; disjoint spacetime blocks per thread, fused downstream; input transport under 10 µs.

A latency of 63 µs against a 1.1 µs cycle deserves a decode of its own: the answer to "what did this logical measurement mean" arrives roughly 57 cycles after the data does. That is affordable only because of the block architecture — the decoder works on 10-cycle blocks with future-cycle fusion, so latency reflects pipeline depth, not a stalled machine. Throughput, the hard gate, stays at one cycle per round throughout. The distinction matters: a deep pipeline with full throughput keeps the backlog at zero, but every feed-forward decision is still being made with tens-of-microseconds-old information — hence the 10 µs reaction-time assumption remaining ahead of the hardware.

The honest asterisks, which the authors state themselves:

- **Accuracy costs speed.** The real-time decoder achieved ε₅ = 0.35% per cycle and Λ = 2.0 ± 0.1, versus the *offline* neural-network decoder's 0.269% and Λ = 2.18 — and that network runs at 24 µs per cycle, 20x too slow to use live. Going real-time cost ~20–30% in logical error: the speed/accuracy trade is now a measured quantity, not a conjecture.
- **Distance 5 only.** The headline distance-7 memory was decoded offline. And 63 µs is still ~6x above the 10 µs reaction-time assumption the resource estimates use; syndrome volume grows with d², so real-time decoding at factory distances (d ~ 15–30) is an open engineering problem.

For calibration, the 2012 projection had estimated ~100 µs per round might be feasible in parallel software — fine for ion traps, hopeless for superconducting hardware, which was already recognized as needing sub-microsecond, likely hardware-assisted decoding. The 2024 result closed that gap in software, at d=5.

## The moving floor: software and hardware decoders

The 63 µs figure is a snapshot, not a limit. On the software side, Sparse Blossom ([arXiv:2303.15933](https://arxiv.org/abs/2303.15933), PyMatching v2) decodes both X and Z of a **distance-17** surface-code circuit in **under 1 µs per round on a single CPU core** at realistic noise — matching superconducting data rates in pure software. Parallelization makes throughput scalable in principle: parallel-window, time-parallel, and modular decoding schemes split the spacetime graph across machines, each paying a small accuracy cost at the window seams — and decoding *across* lattice-surgery operations, where the graph itself changes shape mid-stream, has its own dedicated schemes.

The scaling concern the 2024 authors state themselves: syndrome volume per cycle grows with d², so a factory-distance code (d ~ 27, ~1457 physical qubits per logical qubit) multiplies the per-round data by ~30 over d=5. Closing that is an engineering program, not a paper.

On the hardware side, decoders are moving into FPGAs and ASICs, close to the cryostat:

- Lookup-table and clustering decoders deliver tens of nanoseconds to sub-microsecond per round on FPGAs for small-to-medium codes.
- Riverlane's collision-clustering ASIC decodes a ~1000-qubit patch at MHz rates in **0.06 mm² at 8 mW** — the power argument for silicon next to the fridge — and their local clustering decoder runs under 1 µs per round on FPGA with MWPM-competitive accuracy. They have also demonstrated real-time decoding wired into a partner's superconducting processor, FPGA in the loop.
- Micro Blossom demonstrated the first sub-microsecond *exact* MWPM hardware decoder, fitting distance 13 on a single FPGA; distributed union-find designs trade some accuracy for simplicity and have been credited with real-time operation out to very large distances.

The pattern: per-patch, per-microsecond decoding is a solved problem in several technologies. None of them yet owns the whole loop.

The unsolved part is scale: fast per-patch decoders exist, but nothing sustains real-time decoding plus integrated feedback at the 10⁵–10⁶-qubit system scale. That integration — decoder, control electronics, and compiler all agreeing on the reaction-time budget — is the frontier.

One calibration worth keeping: these budgets are hardware-relative. An ion-trap cycle is milliseconds, not microseconds — a latency budget three to four orders of magnitude larger, where a 2012-era CPU decoder already sufficed. Real-time decoding is a *superconducting* crisis; the physics is universal, but the deadlines are not.

## Tripwires: flag fault-tolerance

One last piece of the real-time picture, and it is literally a tripwire. In a naive syndrome-extraction circuit, a single fault on a measurement ancilla can spread through later gates into a **hook error** — a weight-2 error on the data qubits that halves the code's effective distance. One bad solder joint, two broken tiles.

**Flag qubits** are the tripwire: one or a few extra ancillas entangled with the syndrome ancilla, measured alongside it. A raised flag *heralds* that a hook-inducing fault occurred, triggering a conditional re-measurement or a targeted correction. Flags buy fault tolerance with far fewer qubits than the older verified-ancilla constructions, and they are what make surface-code extraction work on low-connectivity hardware like heavy-hex layouts — though a 2025 comparison found flagged and SWAP-based heavy-hex extraction pay the same threshold price (~0.30%, versus 0.67% on the square lattice), so flags do not repeal the connectivity tax.

The foundational results date to 2018: Chamberland and Beverland showed flag-based extraction works for codes of arbitrary distance, and Chao and Reichardt achieved fault tolerance with as few as two extra qubits total. Flags were also what first made surface-code extraction viable on IBM's heavy-hex connectivity, where each qubit talks to at most three neighbors — the tripwire compensating for a schedule that cannot.

The current best practice has a twist: on the rotated surface code, carefully chosen **unflagged** measurement schedules route CNOTs so that hook errors trigger enough neighboring detectors to be matched correctly anyway — full circuit-level distance with no extra qubits and no flag-measurement time cost. Flags remain the tool of choice where scheduling cannot save you: low-connectivity graphs, color codes, small fault-tolerant experiments, and magic-state preparation.

The reason flags belong in this lesson: a flag is information that exists *only* if someone is listening in time. A tripwire that nobody watches until next week is decorative. Real-time decoding is what makes heralded faults actionable — which is also why cultivation's early-stage checks (same spirit: catch rare faults explicitly, restart while cheap) presuppose a fast classical loop underneath.

## What the compiler needs from the decoder

Close the loop with tier 3: the compiler and the decoder are parties to one contract. The compiler plans circuits reaction-time-limited — runtime = T-depth x reaction time — so it needs the decoder to *output* its reaction-time budget (decode latency plus control-system latency), not merely an error rate. The decoder, in turn, needs the compiler to state which accuracy operating point the resource estimate assumed: offline-quality Λ or real-time-quality Λ, a measured ~20–30% apart on the 2024 device. Throughput below the cycle time is the hard gate; latency is the term in the runtime formula; accuracy is the term in the error budget. A plan that mixes the three up is not conservative — it is wrong.

## Key numbers

- The bar: superconducting cycle ≈ **1.1 µs**; decoder throughput must stay below one cycle per round, sustained — a decoder slower than decoherence is a crash, via the **backlog snowball**. Average speed is not enough; the system must absorb and recover from latency spikes.
- **Feed-forward**: T-gate teleportation needs a decoded logical outcome to choose the next gate; reaction time (decode + control) is the logical clock. Planning assumption: ~10 µs.
- Google 2024 (arXiv:2408.13687): **63 ± 17 µs average latency at d=5**, constant to **10⁶ cycles**; real-time accuracy ε₅ = 0.35%, Λ = 2.0 ± 0.1 vs offline NN Λ = 2.18 at 24 µs/cycle — a ~20–30% accuracy price for speed. d=7 was decoded offline only.
- Software Sparse Blossom: < 1 µs per round at **d=17** on a single core (arXiv:2303.15933); hardware decoders (FPGA/ASIC, e.g. 0.06 mm² / 8 mW ASIC) push below 1 µs at patch scale; system-scale integration is unproven.
- **Flag qubits**: heralded tripwires against ancilla faults spreading to weight-2 hook errors; on the rotated surface code, hook-avoiding unflagged schedules now compete with them.

## Next

You now have all three threads of the modern machine: cheap magic states, a decoder that keeps pace, and plumbing that computes. What remains is the evidence that any of it works on actual hardware — a decade of experiments from five qubits at threshold to a machine below it: [experiments: threshold to below-threshold](#/lesson/experiments). That is the last reading lesson; after it, tier 5 puts the toolchain in your hands, starting with [environment setup](#/lesson/setup-env) — where the decoders and simulators from this tier stop being prose and start being processes on your own machine.
