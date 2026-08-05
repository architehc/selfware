# The compilation pipeline

You now know the two ends of the story: algorithms are made of gates, and fault-tolerant hardware speaks merge/split plumbing. Between them sits a compiler — actually a *chain* of tools, each translating one representation into the next. This lesson walks the whole chain end to end. It is also your map for tier 5, where you will drive several of these tools yourself.

Here is the reference pipeline in one view:

```
quantum circuit (Qiskit / QASM)
  -> ZX graph            (PyZX: spider fusion, simplification)
  -> 3D blockgraph       (TopoLS / topologiq / qelebrimbor;
                          historically: SketchUp by hand)
  -> tqec BlockGraph     (cubes + pipes + correlation surfaces)
  -> Stim circuit        (per code distance, with noise model)
  -> decode & simulate   (Stim + PyMatching; Clifft for non-Clifford)
```

Each arrow buys something specific. Walk them in order — and as you go, keep a running ledger of *what each stage buys*, because that ledger is the real lesson:

- **ZX graph** buys the right primitives: measurements exposed, gates dissolved.
- **Blockgraph** buys geometry: a floor plan, ports, and a volume you can cost.
- **tqec compilation** buys physics: verified plaquette circuits, detectors, observables.
- **Simulation and decoding** buy evidence: a logical error rate, measured, not estimated.

A stage that skips its payment — a blockgraph nobody costed, a circuit nobody decoded — is a claim, not a design.

## Stage 1: circuit to ZX graph (PyZX)

The input is an ordinary quantum circuit — Clifford+T gates on qubits, written in Qiskit or QASM. The first move is to leave the gate picture entirely and translate into a **ZX diagram**: a graph of "spiders" (Z-type and X-type nodes carrying phases) connected by wires, the native language of the ZX calculus, a graphical rewrite system for linear maps on qubits.

Why bother? Because gates hide structure that spiders expose. PyZX, the de-facto substrate for this whole stage, applies rewrite rules — **spider fusion** (adjacent same-color spiders merge), local complementation, pivoting — until the graph is in a reduced, phase-free-ish form. In that form, a multi-body Pauli measurement that was invisible as a gate sequence sits there as one fat spider. Remember the last two lessons: multi-body Pauli measurement *is* the surface code's native instruction. The ZX graph is the circuit re-expressed in the machine's own vocabulary, before anyone has committed to geometry.

The ZX graph does one more quiet service: it is also the *verification* format. Two diagrams compute the same thing if and only if their ZX graphs rewrite to each other, so equivalence checking — did my fancy compressed plumbing really preserve the algorithm? — reduces to graph rewriting, not wavefunction arithmetic.

## Stage 2: ZX graph to 3D blockgraph

Now the hard, not-fully-solved step: inflate the flat ZX graph into the 3D plumbing of the previous lesson — cubes, pipes, ports, minimal volume. Three young tools attack it, and tier 5 will have you run one:

- **TopoLS** ([arXiv:2601.23109](https://arxiv.org/abs/2601.23109)) is the most compiler-like: it slices the ZX graph into layers by connectivity, then searches 3D embeddings with Monte Carlo Tree Search, explicitly minimizing space-time volume. Claimed result: about **33% average volume reduction** versus earlier heuristic compilers, demonstrated up to 500-qubit random circuits. Its output is a pipe diagram consumable directly by tqec.
- **topologiq** converts a PyZX graph into a blockgraph one edge at a time, by heuristic pathfinding: place a spider, enumerate candidate 3D positions, keep only topologically valid paths, score them by length and by how much room they leave for future placements. No global optimization, but the construction is transparent and even animatable — a teaching vehicle as much as a compiler.
- **qelebrimbor** takes a third route: find the graph's cycles, realize the heaviest cycle as a minimal ring of cubes first, then grow the rest outward. Experimental single-author research code — treat its claims as unverified, its existence as a sign of how open the problem is.

None of this is settled science; "ZX graph to minimal plumbing" is an active research front. What matters for you is the interface: all three aim at the same target, the tqec BlockGraph.

It is worth one paragraph on *why* this stage is hard, because the difficulty explains the tool diversity. Embedding a graph in a 3D grid of patch-sized cells sounds like a puzzle game, and at four qubits it is. But every placement decision interacts with every later one — a pipe routed greedily now can wall off a port needed three layers later — and the objective (space-time volume) is global. Exhaustive search explodes; hence the three different heuristics above, none dominant. If this reminds you of place-and-route in classical chip design, it should: the field is reinventing EDA, with correlation surfaces playing the role of netlists.

## The historical front door: SketchUp and .dae

Before any of these tools existed, the blockgraph was drawn *by hand* — in SketchUp, a general-purpose 3D modeller. Austin Fowler's community talk "Programming a quantum computer using SketchUp" is exactly what it sounds like: humans dragging cubes and pipes around a 3D canvas, eyeballing the plumbing, exporting the geometry.

The remarkable part is that this manual path is fully wired into the modern toolchain, and it is the path you will walk first in tier 5. SketchUp exports **Collada (.dae)** files, and tqec ingests them directly:

```
import tqec

graph = tqec.BlockGraph.from_dae_file("logical_cnot.dae")
surfaces = graph.find_correlation_surfaces()   # pick logical I/O
compiled = tqec.compile_block_graph(graph, observables=[surfaces[0]])
circuit = compiled.generate_stim_circuit(k=2)  # distance d = 2k+1 = 5
```

That snippet is the whole bridge from a drawing to a physics-grade circuit. Manual drawing scales terribly — which is precisely the gap TopoLS and friends exist to close — but as a way to *feel* what a blockgraph is, nothing beats building one with your own mouse. The tqec repository even ships a worked example, `assets/logical_cnot.dae`: a hand-drawn logical CNOT that parses, compiles, and simulates — proof that the manual path is not nostalgia but a maintained interface.

Notice also what the call signature tells you about the design: the blockgraph itself carries **no distance and no noise**. Distance enters only at the last moment, as the parameter k with d = 2k+1, and noise as a pluggable model such as uniform depolarizing at 10⁻³. The logical design and the physical instantiation are deliberately separated — one drawing, rendered conservative or aggressive as the question demands.

## Stage 3: tqec BlockGraph to Stim circuit

The tqec library is the anchor of the stack — the only lattice-surgery compiler that emits simulator-ready circuits. Its internal representation is exactly the previous lesson's picture: a `BlockGraph` of cubes (typed XXZ, ZZX, ... by their port bases) and pipes. Compilation is staged and mechanical:

- **Verify**: `find_correlation_surfaces()` enumerates the logical sheets through the graph; you choose which surfaces are the observables you care about. This is the correctness certificate from the last lesson, automated.
- **Substitute**: `compile_block_graph()` replaces every cube and pipe with a pre-verified **plaquette template** — a checked circuit fragment for that block, from a block library.
- **Instantiate**: `generate_stim_circuit(k, noise_model)` renders the compiled graph at code distance **d = 2k+1** (k=1 gives d=3, k=2 gives d=5, and so on) with a chosen noise model, annotating **detectors** (the tier 2 objects) and logical observables as it goes.

The output is a **Stim circuit**: a text file of gates, noise channels, and DETECTOR/OBSERVABLE annotations. Same blockgraph, any distance — that is the point of a compiler. The same logical CNOT can be rendered at d=3 for a quick test or d=15 for a serious estimate.

## Stage 4: simulate and decode (Stim, PyMatching, Clifft)

A Stim circuit is not an answer; it is an experiment you can now run a million times.

- **Stim** simulates stabilizer circuits at ferocious speed (reference-frame sampling: one full simulation, then shots derived almost free) and extracts the **detector error model** — the graph of error mechanisms and the detectors they flip, the standard handoff to a decoder. Two details will matter in tier 5: `decompose_errors=True` breaks hyper-edge errors into graph-like pieces a matcher can digest, and Stim ships with **Crumble**, an interactive circuit editor, for eyeballing what you just compiled.
- **PyMatching** decodes: it builds its matching graph from the detector error model and runs the blossom algorithm you met in tier 2, returning predicted logical flips. Run many shots across distances and noise rates (typically orchestrated with the **sinter** tool, which parallelizes sampling and stops each task once it has enough errors for statistics), and you get the prize plot: logical error rate versus physical error rate, with the threshold crossing and the Λ suppression factor from tier 1.
- **Clifft** ([arXiv:2604.27058](https://arxiv.org/abs/2604.27058)) covers the gap Stim cannot: circuits with genuine non-Clifford content, like magic-state cultivation. It keeps only the currently non-Clifford degrees of freedom in a dynamically sized statevector, so it stays exact where statevector simulators would explode and Stim cannot go at all. You will meet it in tier 5's final hands-on.

This back end is what makes the whole stack *engineering* rather than conjecture: a design is not believed until its compiled circuit has been Monte-Carlo'd below threshold.

## Maturity flags, honestly stated

This pipeline is real and used in published research, but it is young, and tier 5 will go better if you arrive with calibrated expectations:

- **tqec** is solid research software with a journal paper, but its docs warn "under active development, no backwards compatibility" — pin versions, and expect APIs to drift.
- **TopoLS** has a paper, code, and reproducibility scripts, with documented rough edges; the 33% volume figure is the authors' claim, not an independent measurement.
- **topologiq** has no paper yet and warns of breaking changes; **qelebrimbor** is single-author experimental code with no releases. Treat both as demonstrations of an open problem, not finished products.
- Every performance number quoted in this lesson comes from the tools' own papers and READMEs. In tier 5 you will reproduce the small ones yourself, which is the only way numbers in this field should ever be believed.

## One circuit, many distances: the study you will run

The payoff picture, sketched once so tier 5's plots are not a surprise. Take the verified logical-CNOT blockgraph above and render it at k = 1, 2, 3 (distances 3, 5, 7) with a noise model around p = 10⁻³. For each circuit, build the detector error model, hand it to PyMatching, and sample until you have a trustworthy logical error rate per distance. What you should see is the tier 1 scaling law made flesh: below threshold, each step of +2 in distance divides the logical error rate by the suppression factor Λ. On one plot — logical error rate versus physical error rate, one curve per distance — the curves cross at the threshold itself.

That plot is the field's unit of evidence. The Google below-threshold papers of tier 4 are exactly this study, run on hardware instead of in Stim; your tier 5 version is the same methodology at laptop scale. When a factory or compiler paper claims a better design, this plot is where the claim must eventually land.

## Why the pipeline matters to you

Step back and notice the shape of the chain: **each stage is a bet about what to make explicit.** The ZX graph makes measurement structure explicit; the blockgraph makes geometry and volume explicit; the Stim circuit makes every physical gate, error, and detector explicit. Compilation is the art of delaying commitment until each decision can be made well.

Tier 5 turns this lesson into muscle memory. You will set up the environment, then run three hands-on sessions: draw and compile a blockgraph with **tqec** all the way to a logical-error-rate plot, compile a circuit to plumbing with **TopoLS**, and simulate a cultivation circuit with **Clifft**. Every step of the diagram at the top of this lesson becomes a command you have typed.

## Key numbers

- tqec renders a blockgraph at distance **d = 2k+1** via `generate_stim_circuit(k)`; SketchUp designs enter via `BlockGraph.from_dae_file(path)`.
- TopoLS claims ~**33%** average space-time-volume reduction over prior heuristic compilers (authors' figure, arXiv:2601.23109).
- The decoder handoff is Stim's **detector error model**; PyMatching's sparse blossom is the tier 2 algorithm, industrialized.
- Stim covers Clifford+Pauli-noise only; **Clifft** extends exact simulation to near-Clifford circuits like cultivation.

## Next

One ingredient has been waved at repeatedly — the magic states that feed the T gates, distilled in factories that dominate the machine's footprint. Tier 4 opens with them: [magic states and distillation](#/lesson/magic-states).
