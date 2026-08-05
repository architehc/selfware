# Space-time plumbing and blockgraphs

You left the last lesson with a one-line instruction set: merge, split, measure. Now run a whole algorithm — thousands of merges and splits on hundreds of patches — and ask the obvious question: *what does the program look like?* Nobody writes such a schedule as a list. They draw it, in 3D, as plumbing. This lesson teaches you to read the drawing.

## Time is the third axis

Take the 2D chessboard floor plan from the last lesson — patches as tiles on a grid — and let time point upward. A patch that sits still for a while, storing its logical qubit, sweeps out a square column: a **pipe**. Its cross-section is one patch, its height is how long the qubit lives.

Now add computation:

- A **merge** is a junction where two pipes join into one — a tee fitting.
- A **split** is a junction where one pipe forks into two.
- A **measurement** or initialization is a cap: a pipe ending against a wall.

The whole algorithm becomes a connected 3D arrangement of pipes and junctions — literally a plumbing diagram. Here is a CNOT's skeleton, time running upward:

```
time
  ^
  |    output C        output T
  |      |               |
  |      |             (cap: readout)
  |      |               |
  |      |            (rough merge: X_L X_L)
  |      |               |
  |    (split)        INT pipe
  |      |   \           |
  |      |    (smooth merge: Z_L Z_L)
  |      |           /
  |    input C    INT in |+>
```

This object goes by several names: **space-time diagram**, **pipe diagram**, or, in the tqec toolchain, a **blockgraph**: a graph whose nodes are **cubes** (junctions and storage blocks) and whose edges are **pipes** (connections between cubes). When Fowler says you can program a quantum computer in the SketchUp 3D modeller, this is what he means: you drag cubes and pipes around until the plumbing matches your algorithm.

The analogy earns its keep immediately. Every pipe face is **typed** by the boundary it exposes — X or Z, the smooth/rough ports from tier 1. A joint measurement is legal only where pipes expose compatible ports, exactly as a fitting only connects matching threads. Measuring a mixed-basis operator may require rotating a block first to reorient its ports — the plumbing equivalent of adding an elbow joint.

One genuinely strange liberty comes with the picture: pipes can validly be routed *backward in time*. A pipe that loops down and back up is not science fiction; it describes teleportation-style tricks where a measurement now and a preparation later stand in for a wire. The diagram does not care which way is "forward" — only the causal dependencies of the measurement outcomes do. This freedom is the seed of the time-optimal trick at the end of the lesson.

A reading tip that prevents most beginner confusion: pipes in these diagrams can run along any axis, including the time axis itself. A pipe lying *horizontally in time* — extending sideways across the floor plan — is a spatial connection between distant tiles happening in one interval; a pipe running straight *up* is a qubit sitting still in memory. Same object, different orientation, very different meaning. When a diagram confuses you, first ask of every pipe: is this storage, or is this communication?

## Correlation surfaces: the correctness certificate

A plumbing diagram is a drawing; why should anyone believe it computes the right thing? The answer is the deepest idea in the subject, and it completes a thread from tier 1.

Recall that a logical operator is a *string* spanning a patch between same-type boundaries. In 2D + time, that string sweeps out a 2D sheet. A **correlation surface** is exactly that: a sheet dragged through the blockgraph, entering through one pipe's port, flowing through junctions, and leaving through another. It is the worldsheet of a logical operator — the logical X or Z of your computation, tracked through every merge and split.

The validity condition, inherited from the RHG construction of 2005-2007 and used by every tool since:

> A space-time diagram is valid if and only if its correlation surfaces connect the intended input ports to the intended output ports, carrying the right logical operators.

Everything else is bookkeeping. When a correlation surface splits at a junction, the outcome lands as a byproduct in the Pauli frame — the classical scoreboard again. So a compiler's verification job is concrete: enumerate the surfaces, check the connectivity. The tqec library exposes this directly as `find_correlation_surfaces()`. If you remember one sentence from this lesson, make it the quote above: it is what "correct" means for an entire field.

Surfaces also make the last lesson's algebra *visible*. At a rough-merge junction, the two incoming pipes each carry an X_L sheet; inside the merged pipe those two sheets have fused into one sheet that spans the weld. The surface picture and the stabilizer picture are the same fact in two languages: the weld identifies the two logical X operators, which is precisely what "the merge measured X_L X_L" means. When a tool checks your diagram, it is checking that every sheet you intended to fuse actually fuses, and none fuse that should not.

## Reading a factory block

The plumbing diagrams in the literature come in two drawing styles, and confusing them causes real misreadings. A **to-scale** drawing shows every block at its true d³ size. An **exaggerated-spacing** drawing ([arXiv:1812.01238](https://arxiv.org/abs/1812.01238)) pulls the pipes apart to show connectivity clearly, then annotates each block with its true footprint and depth — the factory papers label blocks with dimensions like 12d x 6d and durations like 5.5d cycles, or mark an adder's MAJ block as a 3x3x5 bounding box with named ports. Read exaggerated-spacing diagrams for *topology*, and take the *cost* from the annotations, never from the whitespace.

Those annotated blocks are how the field communicates designs: a factory is published as a bounding box, a throughput (one output state per so-many d cycles), and an output error rate. When tier 4 quotes a 15-to-1 T factory as 12d x 8d x 6.5d, it is speaking this block-diagram language — three numbers that fit on a slide and determine the fate of a machine.

## Delayed choice: pipes with undecided caps

One more plumbing fixture appears in the modern diagrams, and it is pure quantum strangeness put to engineering use. A **routing qubit**, nicknamed a **chimney** ([arXiv:1905.08916](https://arxiv.org/abs/1905.08916)), is a vertical column left open in the diagram — a pipe whose top is deliberately uncapped. Later, when the classical control system knows what the algorithm needs, the chimney is capped by an X or Z measurement, chosen on the fly.

The payoff is real estate. Fowler's 2012 multiplexed-routing CZ needed 8 routing qubits to steer around congestion; the delayed-choice CZ needs **2** — a 4x volume cut — because the decision is postponed until the path is known rather than hedged in hardware. Stack three routing qubits into one CCZ state and you get the AutoCCZ, a block so polite it "cleans up its own fixup garbage": the corrections that would normally chase the computation downstream cancel inside the block itself.

The pattern to file away: *postpone every decision that classical information can settle later.* Open pipes are cheap; committed pipes are expensive. This is the same instinct as the Pauli frame — never do physically what bookkeeping can do — expressed in plumbing.

## Volume is the currency

Now the economics. Every cube of the diagram — a patch-sized block, d on a side, persisting for one block of time — is a chunk of hardware multiplied by time. The natural unit is the **d³ block**: one patch's worth of qubits, kept for one patch's worth of rounds. The total **space-time volume** of a diagram, counted in d³ blocks, is the single number that predicts what a computation costs: qubits x time, the resource you are always short of.

Once volume is the currency, compilation becomes an optimization problem: *deform the plumbing to spend fewer blocks, without changing what it computes.* The founding result is Fowler and Devitt's **bridge compression** ([arXiv:1209.0510](https://arxiv.org/abs/1209.0510), 2012). They developed a toolkit of topology-preserving deformations — pushing in bumps, swapping primal and dual segments, cancelling double braidings — and proved each deformation legal by exhibiting the correlation surfaces before and after. Same computation, smaller plumbing. Their benchmarks, in d³-scaled volume units:

- The **15-to-1 magic-state distillation** circuit (the factory that turns 15 noisy T-states into one clean one — tier 4's subject) compressed to a volume of **192** units, from parts costing 16 minimum-volume CNOTs of 12 units each.
- The smaller 7-to-1 distillation compressed to **18**.
- Chaining two distillation levels (15-to-1 feeding 15-to-1, output error 35(35p³)³) cost **552** units — and the paper argued you rarely need it: halving the input error rate improves the two-level output by more than 500x, so one good level plus decent injection covers even trillion-gate computations.

This was the first time anyone treated a fault-tolerant circuit as an object to be *compressed*, and it fixed the target every later compiler aims at: the 15-to-1 volume is the baseline that modern factory designs (and tier 4's cultivation) claim to beat. Notice what is being optimized — not gate count, not depth, but *plumbing volume*.

## Time-optimal computation: teleport the latency away

Volume is cost, but what about *speed*? A naive diagram executes layers one after another, and each merge costs d rounds, so runtime grows with the depth of the circuit multiplied by d. Fowler's **time-optimal quantum computation** ([arXiv:1210.4626](https://arxiv.org/abs/1210.4626)) demolished that assumption with the backward-in-time pipes you met above.

The trick is selective teleportation. Prepare the ancilla states a computation will need *in advance*, in parallel, all across the diagram; then, when the algorithm reaches a gate, consume the pre-made state by teleportation — a single round of measurement whose only real-time cost is one measurement cycle plus the classical feed-forward to interpret the outcome. All the slow, distance-scaling work was done earlier, offline, paid in volume (extra pipes living longer) rather than in latency.

The result is a startlingly clean formula:

> The runtime of a time-optimal computation is its **T-depth x one measurement time**. Everything else is paid in space-time volume.

T-depth is the number of layers of non-Clifford T gates in the algorithm — the one thing that cannot be prepared ahead, because each T layer's corrections depend on the previous layer's measurement outcomes (the reaction-time limit from tier 2's streaming story). Clifford gates vanish from the runtime entirely: they are absorbed into the Pauli frame, handled by the classical control system, costing zero quantum time. The compiler's objectives are now fully specified, and they are still the objectives today: minimize T-count and T-depth (they set latency), then minimize volume (it sets the qubit bill).

## Where the volume goes

A sane question at this point: in a real machine, which pipes dominate the budget? Not the data. The defect-era resource estimates ([arXiv:1208.0928](https://arxiv.org/abs/1208.0928)) put about **94% of the machine's qubits inside distillation factories** — the blocks that manufacture clean magic states — with a 2000-bit Shor algorithm costing around 10⁹ physical qubits in that accounting. The algorithm's own data pipes are a rounding error next to the supply chain that feeds it.

That lopsidedness is the single most important fact for reading diagrams: when you open a modern space-time figure, the sprawling structure is mostly factories and routing, and the "computation" is a thin thread running through it. It also tells you where optimization pays. A 20% smaller algorithm layout barely moves the total; a 20% smaller factory redesigns the machine — which is why bridge compression aimed at distillation first, and why tier 4 spends two full lessons on the factory supply chain and its modern replacement, cultivation.

## From picture to artifact

These diagrams began as whiteboard art — the "exaggerated-spacing" sketches in the Gidney-Fowler factory papers, drawn for connectivity rather than scale, annotated with footprints like 12d x 6d and depths like 5.5d cycles. But a drawing you can cost and verify is a drawing you can *compile to*, and that is what happened: the blockgraph became a machine-readable intermediate representation, with cubes typed by their port bases, pipes connecting them, correlation surfaces as the proof obligation, and d³ volume as the score.

The artifact also fixes the division of labor you will see in every modern paper: the diagram is the *logical* design, independent of code distance; distance, noise, and the physical circuit enter later, at compile time. And one thread is deliberately left hanging for tier 4: time-optimality makes the runtime hostage to classical reaction time — every T layer waits on measurement results and their interpretation. How fast a decoder can answer, in microseconds, stops being an engineering footnote and becomes the speed of the computer. The next lesson is about the software that produces and consumes these artifacts — and about who draws the plumbing when the algorithm is too big for a human with a 3D modeller.

## Key numbers

- A computation in 2D + time is a 3D **blockgraph**: **cubes** (junctions/patches) joined by **pipes**; every pipe face is a typed port (X or Z).
- Correctness = **correlation surfaces** connecting intended input ports to intended output ports; byproducts go to the Pauli frame.
- Cost unit: the **d³ space-time block**. Compilation = volume compression under topology-preserving deformations.
- Bridge compression (arXiv:1209.0510): 15→1 distillation compressed to **192 d³-units** (from 16 CNOTs at 12 each); 7→1 to 18.
- Time-optimal computation (arXiv:1210.4626): latency = **T-depth x one measurement time**; all other cost paid in volume. Cliffords are free at runtime.

## Next

Who draws the plumbing, checks the surfaces, and turns cubes and pipes into a million-line circuit for a simulator? A software stack — and it has a surprisingly manual front door. On to [the compilation pipeline](#/lesson/compilation-pipeline).
