# Magic states and distillation

Tier 3 ended with a full compilation pipeline: circuit to ZX graph to plumbing to Stim circuit. One ingredient was waved through without explanation — the **T gate**. It sits in every interesting algorithm, it cannot be done by lattice surgery, and supplying it dominates the cost of the entire machine. This lesson is about that ingredient: why it is needed, why it arrives dirty, and the refinery that cleans it.

## Why Clifford gates are not enough

Every operation you have seen so far — X/Z initialization and measurement, Hadamard, S, CNOT, merges and splits — is a **Clifford** operation: it maps Pauli operators to Pauli operators. Clifford circuits have a stunning property, the **Gottesman–Knill theorem**: they can be simulated efficiently on a classical computer. Not "simulated with effort" — a laptop tracks a million-qubit Clifford circuit by updating a table of Pauli labels, which is exactly what Stim does. No quantum speedup is possible with Cliffords alone; if your computation were all Clifford, you would not need the quantum computer.

There is a deeper constraint too: the Eastin–Knill theorem says no 2D topological code can offer a *universal* gate set through protected (transversal) gates alone — some ingredient must always enter through an unprotected channel. The surface code is living proof: its protected menu is Clifford-only.

Classical simulability has a practical corollary you have already been using without noticing: Clifford corrections need never be executed on hardware at all. They are tracked as **byproduct operators** in the control software and commuted through the circuit until measurement — "applied in software," as the literature puts it. This is why the Pauli frame from tier 2 exists, and it is why the only operations that truly stress the classical control loop are the non-Clifford ones — a thread the real-time decoding lesson picks up.

The standard fix is to add one non-Clifford gate, the **T gate** (a π/4 rotation about the Z axis, sometimes called the π/8 gate). Clifford + T is universal: any quantum algorithm can be compiled into that set. But a T gate cannot be performed on a protected patch directly. Instead, it is *consumed* in resource form, as a specially prepared qubit called a **magic state**:

```
|T> = (|0> + e^{i pi/4} |1>) / sqrt(2)
```

One |T⟩ state, plus Clifford operations and a measurement, buys you exactly one T gate on a data qubit. The literature uses a few equivalent resource states — |Y⟩ for S-gate corrections, |CCZ⟩ for Toffoli gates — but |T⟩ is the canonical currency. The entire question of fault-tolerant non-Clifford computation reduces to a supply problem: *manufacture clean |T⟩ states, fast enough, in bulk.*

## Injection: every magic state is born dirty

Here is the trap. To get a |T⟩ state into a surface-code patch, you must first prepare it on a **single physical qubit** and grow the code around it — a process called **state injection**. And a single physical qubit has no error correction at all. The 2008 cluster-state paper ([arXiv:0805.3202](https://arxiv.org/abs/0805.3202)) is blunt: since injection always begins with one unprotected qubit, it is *inherently non-fault-tolerant*. The defect-era resource bible ([arXiv:1208.0928](https://arxiv.org/abs/1208.0928)) assumed an injection error around 0.5%; the 2018 factory paper ([arXiv:1812.01238](https://arxiv.org/abs/1812.01238)) still assumed level-0 injected states with error around 2 x 10⁻³, at a physical gate error of 10⁻³.

Compare that with what an algorithm needs. A serious computation — say, factoring a 2000-bit number — consumes trillions of T gates, so each magic state must fail with probability below ~10⁻¹³ or so. Injection delivers 10⁻³. That is a gap of ten orders of magnitude, and no amount of surface-code distance around a dirty state fixes it: the error is already inside, baked in at birth.

Two technical footnotes are worth keeping. First, not every state can be injected: the gates used to grow the code around the seed commute only with Z-basis rotations, so injectable states are limited to the equatorial family (|0⟩ + e^{iθ}|1⟩)/√2 — which fortunately includes both |Y⟩ and the |T⟩ state (also written |A⟩). Second, the defect-era construction injected through a "short qubit" — a deliberately tiny logical qubit whose protection is enlarged to full distance only after the state is in — and the paper is explicit that the result is "necessarily imprecise." The imprecision is not sloppiness; it is the price of the seed moment existing at all.

Think of it as a water supply. Injection is a well that produces contaminated water, every time, at a known contamination level. You cannot clean the well. So you build a treatment plant instead.

## Distillation: sacrifice fourteen, keep one pure

The treatment plant is **magic state distillation**: a circuit, built entirely from protected Clifford operations and measurements, that takes many dirty magic states and produces a few much cleaner ones — accepting the output only when its internal checks pass. It is a probabilistic, post-selected process: many noisy states in, one better state out, kept only on detected success.

The workhorse protocol for |T⟩ is **15-to-1**, based on the Reed–Muller code. Feed in **15** noisy magic states (each consumed by a T† gate inside the circuit), then check the code's four X-stabilizers; if all checks pass, keep the **1** output state. The other fourteen are spent — burned as fuel to purify the fifteenth. (A smaller sibling, 7-to-1 based on the Steane code, does the same for |Y⟩ states with seven inputs.) The arithmetic of the sacrifice ([arXiv:1208.0928](https://arxiv.org/abs/1208.0928), Fig. 33):

- Input error p goes in; output error **35 p³** comes out. At p = 10⁻³, that is ~3.5 x 10⁻⁸ per round of distillation.
- Why cubic? Any single input error is caught by the checks; a bad output requires at least three input errors landing in conspiring places.
- The price is rejection: the protocol succeeds with probability **1 − 15p**, so at p ~ 1% about **one run in six is discarded** and must be rerun. Distillation is a post-selected factory: you pay for the rejects too.

Notice what is *not* expensive here: time. The checking circuit's multi-target CNOTs execute in the same cycles as one CNOT in the surface code, so a 15-to-1 round costs at most about eight CNOT-times. The cost is **space** — the qubits for fifteen inputs, their ancillas, and the checking machinery.

The smaller 7-to-1 protocol for |Y⟩ states is worth a glance for calibration: seven inputs, output error 7p³, success probability 1 − 7p — and two rounds of it (49 inputs total) already reach ~10⁻¹⁵ at p ~ 1%. The 15-to-1 became the standard |T⟩ workhorse because its checks map cleanly onto surface-code plumbing, not because the ratio is magic. Both protocols share the template: entangle against a logical ancilla block, consume the noisy states in gates, verify the code's checks, keep or discard.

One sentence on why this whole approach beat the alternatives. Older concatenated-code schemes pay a volume growing like 1000^L with the number of levels L; topological codes pay a polynomial in the factory size. Distillation on a topological substrate was never the cheapest imaginable scheme — it was the cheapest scheme compatible with the only code family that scaled. That is the soil the factory economy grew from.

## Recursion: distilling the distilled

One round not clean enough? Distill the outputs again. Two levels of 15-to-1 give error 35(35p³)³ — at p = 10⁻³ that is roughly 10⁻¹⁵, below the trillion-gate target. The bridge-compression analysis ([arXiv:1209.0510](https://arxiv.org/abs/1209.0510)) compressed a 15-to-1 circuit to a space-time volume of 192 d³-units and concluded you will *never need more than two levels*: at p = 0.01 the two-level map already reaches ~10⁻¹².

The worked example from the 2012 paper (Shor-2000, physical error 10⁻³) shows the shape of a real factory:

- **Level 1** runs at distance d₁ = 17: fifteen distillation circuits in parallel, ~8 x 10⁵ physical qubits, output error ~4 x 10⁻⁶.
- **Level 2** runs at distance d₂ = 34: ~2.4 x 10⁵ physical qubits, output error ~3 x 10⁻¹⁵.

Note the discipline in those distances: run the *early*, dirty stage at *lower* code distance, because there is no point protecting a state better than its own dirtiness. Distillation suppression only has to beat the code's own logical error rate — the factory spends protection exactly where the error budget demands it. The result is a stepped pyramid, level 1 forming the wide base, and the base is the largest single footprint in the whole machine.

## Getting the state into the data: gate teleportation

A distilled |T⟩ sitting in a factory does nothing by itself. It is applied to a data qubit by **gate teleportation** — a close relative of the merge-and-measure moves you know from lattice surgery. Entangle the magic state with the data (a controlled-NOT), measure the data qubit, and the T gate's effect lands on what remains. Half the time, the measurement outcome says an extra **S correction** is needed — a Clifford fixup that depends on classical information arriving in time.

Two things to file away here:

- The Clifford parts of the fixup are tracked in the **Pauli frame** — the classical scoreboard from tier 2 — and never applied physically. Bookkeeping, not pulses.
- The *decision* itself is genuinely real-time: which correction the algorithm needs depends on a decoded measurement outcome. That dependency is the seed of a later lesson, real-time decoding — the reason a decoder's latency sets the logical clock speed of the whole machine.

## The modern factory: leaner, same shape

A decade of engineering — better injection, lattice-surgery layouts, catalysis — made the factory far leaner without changing its shape. The modern reference design ([arXiv:1812.01238](https://arxiv.org/abs/1812.01238), Gidney–Fowler 2018), still at physical error 10⁻³:

- A **level-1 T factory** (15-to-1 done by lattice surgery, at half code distance) produces one |T⟩ per 3.25d cycles and discards only ~3% of runs, with output error ~10⁻⁶-class at minimal distances.
- A **CCZ factory** fed by five level-1 T factories fits a 12d x 6d footprint and emits one |CCZ⟩ (which does the work of four |T⟩ in a Toffoli) every ~5.5d cycles, at ~10⁻¹¹-class error — enough for ~10¹⁰ states before one failure.
- A catalysis trick (**C2T**) recycles a |CCZ⟩ output as a catalyst to convert eight noisy |T⟩ into two clean ones with quadratic suppression — shrinking the combined factory footprint by another 25%.

Two operational details matter for the plumbing picture from tier 3. First, discarded runs are not free: a factory that fails ~3% of the time needs *buffering* — surplus states parked in routing hallways so a failed run never stalls the algorithm. Second, factory outputs exit through one port, so a fast-consuming algorithm can be limited by *routing*, not production: in the CCZ design the exit is occupied 3d of every 5.5d cycles, leaving 2.5d for everything else. Factories are supply-chain infrastructure, with supply-chain problems.

One catalysis caveat is a beautiful example of how subtle factory engineering gets. The C2T catalyst is *reused*, so an error in it is not independent run to run — it is correlated across every state the catalyst touches. The design consequence: catalyzed stages go **last** in a distillation chain, where one bad output fails the whole algorithm anyway — which turns the correlation from a bug into a quadratically smaller whole-run failure rate. Placement is error-budget judo.

For scale calibration: in the 2012 accounting, one full two-level |A⟩ factory was ~800,000 physical qubits producing about two states per 500 cycles, and feeding Shor-2000 needed ~1200 of them. The 2018 designs brought the same output down to blocks measured in dozens of d on a side — the decade's progress in one sentence.

## How to read a factory claim

Factory papers communicate in the block-diagram language of tier 3: a bounding box (12d x 6d), a throughput (one output per 5.5d cycles), and an output error rate. Three habits will keep you from being fooled:

- **Check what distance the output lives at.** A state quoted at factory distance is not a state ready for a d ~ 30 computation; the growth cost is real and was often omitted.
- **Check the noise assumption.** Every number in this lesson assumed physical error ~10⁻³ with a specific injection scheme; change either and the rankings can shuffle.
- **Check what is counted.** Qubit-rounds including rejects and buffering, or just the happy path? The gap between those two accountings is where optimistic headlines live.

## The factory economy: 90–94% of the machine

Now the number that shapes the field. In the 2012 resource estimate for factoring a 2000-bit number ([arXiv:1208.0928](https://arxiv.org/abs/1208.0928)), the machine cost ~10⁹ physical qubits — and the 4000 computational logical qubits, where the actual algorithm lives, totaled about **6%** of them. The other **~94% was distillation factories**: the supply chain, not the computation.

The 2018 redesigns cut the total dramatically, yet even there factories still occupy on the order of **90%** of the machine. The lopsided economy did not change; it got cheaper. When you open a modern space-time diagram, the sprawling structure is mostly factory and routing, and the "computation" is a thin thread running through it.

Notice also that better hardware alone does not fix the ratio. Improving the physical error rate tenfold (10⁻³ to 10⁻⁴) shrank the 2012-style machine from ~10⁹ to ~1.3 x 10⁸ qubits — with factories *still* dominating, because better hardware shrinks data patches and factories proportionally. Only a better factory changes the ratio.

## What distillation cannot fix

By 2024 the field had a clear-eyed list of distillation's structural inefficiencies — the things no amount of engineering inside the batch model would remove:

- **The dirty seed.** Every pipeline starts from an unprotected single-qubit state at ~10⁻³–10⁻² error, then pays 15x qubit blowups per level to cube the error away. The seed moment is never eliminated, only amortized.
- **Discrete distance jumps.** Multi-level factories run whole floors at fixed distances chosen to barely meet the budget (17, then 34), over-protecting fifteen noisy inputs at a time because the architecture cannot spend protection gradually.
- **Discard-and-buffer overhead.** Post-selection means rejected runs (~1 in 6 per 15-to-1 round at p ~ 1%), and rejected runs mean surplus states parked in routing hallways so a failure never stalls the algorithm — pure routing overhead that someone must place and pay for.
- **Growth was ignored.** Older estimates quoted the cost of *making* a clean state at factory distance and quietly skipped the cost of growing it into the full-distance patch the algorithm actually uses — a bookkeeping gap the next lesson's authors made a point of closing.

Hold this list; the cultivation protocol reads as if it were written by going down it item by item.

This asymmetry tells you where research leverage lives. Shrink the algorithm's layout by 20% and you shave the rounding error. Shrink the *factory* by 20% and you redesign the machine. That is why distillation ate a decade of the community's attention — and why the next lesson's idea, cultivation, which attacks the factory model itself, landed like a thunderclap in 2024.

## Key numbers

- Clifford circuits alone are classically simulable (Gottesman–Knill) — the **T gate** / |T⟩ magic state is the standard non-Clifford supplement that makes the gate set universal.
- **Injection is dirty by construction**: it starts from one unprotected physical qubit, with typical injected-state error ~10⁻³–10⁻² — versus the ~10⁻¹³ per-gate error serious algorithms demand.
- **15-to-1 distillation**: 15 noisy |T⟩ in, 1 clean |T⟩ out; output error **35p³** (cubic suppression), success probability **1 − 15p** (~1 in 6 runs rejected at p ~ 1%).
- Two levels: 35(35p³)³ ~ 10⁻¹⁵ at p = 10⁻³ — enough for trillion-gate algorithms; early levels run at lower code distance to balance the error budget.
- Factories are **~90–94% of the machine** (94% in arXiv:1208.0928's Shor-2000 accounting; ~90% in the leaner arXiv:1812.01238 designs). The data patches are the rounding error.

## Next

Distillation's costs are structural: it over-protects fifteen dirty inputs at fixed distances, throws away rejected runs, and starts from an unprotected seed. In 2024, Google proposed growing a magic state *inside* a small code instead — checking and expanding it in place, spending protection only as quality is earned. That is [magic state cultivation](#/lesson/cultivation). And in tier 5's final hands-on, you will simulate exactly such a cultivation circuit yourself with Clifft — the numbers in the next lesson are the ones you will reproduce.
