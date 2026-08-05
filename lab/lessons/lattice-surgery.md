# Lattice surgery: merge and split

Everything so far has been memory: one patch, one logical qubit, kept alive. This lesson is where the computer arrives. The surprise is that you already own the only tool you need — boundaries. Computation on surface codes is done by *rearranging boundaries*: welding two patches together and cutting them apart again. That is **lattice surgery**, and its operations are called **merge** and **split**.

## From braiding to welding

The original scheme for surface-code computation (the holes you previewed in tier 1) works by **braiding**: dragging hole-shaped defects around each other so their worldlines tangle, the way braiding anyons performs logic. It is beautiful topology and terrible engineering. Braided layouts at distance 3 need over a hundred physical qubits for one logical CNOT, and the pipes that defects travel in eat most of the chip.

In 2012, Horsman, Fowler, Devitt, and Van Meter ([arXiv:1111.4022](https://arxiv.org/abs/1111.4022)) proposed to skip the braiding entirely. Leave every patch bolted in place. To make two patches interact, fill the gap between them with a row of fresh qubits, switch on new checks spanning the seam, and let the two chessboards fuse into one bigger board. Then cut them apart again along the same seam. Welding and cutting chessboards — no moving parts. The paper showed this halves small-scale qubit costs, and by 2018 the field's verdict ([arXiv:1808.06709](https://arxiv.org/abs/1808.06709)) was blunt: defects and braiding should be deprecated. Everything since is surgery.

## Merge = joint parity measurement

Here is the key move, and it is worth reading twice. Take two patches, each storing one logical qubit, parked side by side with a one-qubit-wide gap. Fill the gap with **intermediate data qubits** in a known state, then start measuring the checks that span the seam, for **d rounds** (d rounds, because a check result you have only seen once cannot be trusted — the same noisy-measurement logic as tier 2).

Now ask: what do the new seam checks multiply to? The product of the new seam stabilizers is exactly the product of the two patches' logical operators — X_L on the first patch times X_L on the second. Measuring the seam *is* measuring the joint logical parity X_L X_L:

- A **rough merge** (welding along the rough boundaries, intermediate qubits initialized to |0>) measures **X_L X_L**: are the two logical qubits equal or opposite in the X basis?
- A **smooth merge** (along smooth boundaries, intermediate qubits in |+>) measures **Z_L Z_L**: same question in the computational basis.

The geometry, flattened into ASCII — two patches, a gap of intermediate qubits, and the new seam checks whose product is the joint logical operator:

```
  patch 1        seam         patch 2
+---------+   i  i  i  i   +---------+
| x x x x |   new checks   | x x x x |
| x x x x |  <---------->  | x x x x |
| x x x x |  span the gap  | x x x x |
+---------+   i  i  i  i   +---------+
   X_L x X_L = product of the seam checks
```

The seam is also the delicate spot. If an odd number of seam syndrome bits flip in a way the decoder misreads, no physical correction is issued — instead a reference logical chain is chosen and the defect is *tracked in software* through every later operation. This is the Pauli-frame bookkeeping of tier 2 promoted to a design principle: the seam is allowed to be noisy because the classical scoreboard is exact.

So a merge is not a gate in the usual sense — it is a **measurement**. Two logical qubits go in; one merged patch comes out, carrying the parity of the two inputs, plus one classical bit: the measurement outcome M. That outcome is not corrected with physical pulses. It is written into the **Pauli frame**, the software scoreboard from tier 2, and every later result is interpreted against it. The weld yields information; the information is the point.

Two properties deserve emphasis:

- **The distance survives.** Welding along a seam does not shorten any spanning string, so a merged patch is still distance d. The seam itself is protected by the d rounds of measurement.
- **The outcome is random but tracked.** You cannot choose M; you can only know it. A computer built from measurements must therefore be constantly steered by classical feed-forward — the decoder and the Pauli frame from tier 2 are not accessories, they are half the machine.

## Split: the cutting half

The **split** is the exact converse: measure out a row of data qubits down the middle of a patch (X basis for a smooth split, Z basis for rough), dividing one board back into two. Splits have two personalities worth knowing:

- A smooth split of a patch in state a|0> + b|1> yields two patches in a|00> + b|11> — **the daughters are born entangled**, a Bell pair for free. Split repeatedly and you get GHZ states. Entanglement generation, which is exotic in most architectures, is a side effect of cutting here.
- A split can halve the protection: cutting a square d x d patch gives daughters of distance about d/2 along the cut axis. The fix is to start from a d x 2d rectangle, so each daughter comes out at full distance d. Rectangles are the price of safe childbirth.

## A CNOT from three measurements

If merges and splits are measurements, where are the gates? Answer: gates are *measurement patterns*. The 1111.4022 CNOT uses one extra ancilla patch INT prepared in |+>_L:

- **Step 1 — smooth-merge** the control C with INT (measures Z_L Z_L).
- **Step 2 — smooth-split** them apart: C and INT are now entangled in a|00> + b|11>.
- **Step 3 — rough-merge** INT with the target T (measures X_L X_L), then read out INT.

Walk the algebra through and the net effect on C and T is exactly a CNOT, with the measurement outcomes absorbed into the Pauli frame. Naively that is 3 operations x d rounds each, but two tricks collapse it: preparing the control as a d x 2d patch lets a split stand in for the first merge, and the split commutes with the second merge — so the whole CNOT costs **d rounds** of syndrome time, the same as the old braided CNOT at half the qubit count.

Fowler and Gidney generalized this in 2018 ([arXiv:1808.06709](https://arxiv.org/abs/1808.06709)): **multi-body, mixed-basis Pauli measurements** — measure X_L Y_L Z_L on three patches at once if you like — are the native instruction, with CNOT, S, and T all derived gadgets. A CNOT from two measurements costs 2d of total time. One scheduling wrinkle: a patch can only be measured through the ports it exposes, so measuring a mixed operator may require *rotating* a patch first to bring the right boundary type to the seam. Port orientation is a compile-time constraint; the next lesson's diagrams exist largely to keep track of it.

## Rotated patches: the qubit-count story

The same paper introduced a deceptively simple upgrade you have been looking at all along. The widgets in this lab draw **rotated patches**: the chessboard tilted 45 degrees, with the corners cut off. The distance is unchanged — the shortest spanning string still has length d — but the fat is gone:

- Unrotated patch at distance d: d² + (d−1)² data qubits (roughly 2d² counting measure qubits).
- **Rotated patch: exactly d² data qubits** (plus d² − 1 measure qubits). At distance 3: 25 physical qubits unrotated becomes **13 rotated** — 9 data plus 4 reused central measure qubits. Nearly half the machine, deleted by a rotation.

Stack the savings and the headline numbers of the era appear. The smallest 2D-nearest-neighbor logical CNOT at distance 3 costs:

- **143 physical qubits** with braiding (defect qubits),
- **104** with single defects on a rotated lattice,
- **53 with surgery on rotated patches** (33 data + 20 syndrome qubits) — half of any prior 2D construction.

At architecture scale ([arXiv:1808.06709](https://arxiv.org/abs/1808.06709), [arXiv:1905.08916](https://arxiv.org/abs/1905.08916)) the accounting unit becomes the **tile**: logical-qubit-sized squares on a 2D grid, some holding data patches, some left empty as **ancilla/bus tiles** for merges to occupy, some reserved as routing hallways. Tiled rotated storage with workspace corridors costs **3d² physical qubits per logical qubit** to leading order, versus ~12.5d² for the old double-defect packing — the 4x saving that made the patch-based stack the default architecture.

## Layouts: data tiles, bus tiles, hallways

One level up from the single merge, an entire chip is a tiled floor plan, and the tiles have job descriptions ([arXiv:1808.06709](https://arxiv.org/abs/1808.06709), [arXiv:1905.08916](https://arxiv.org/abs/1905.08916)):

- **Data tiles** hold the logical qubits — one rotated patch each, d² data + d² − 1 measure qubits.
- **Ancilla (bus) tiles** are deliberately left empty so that merges between neighboring data tiles have somewhere to happen. A parity measurement occupies its bus region for d rounds, then releases it.
- **Routing hallways** are corridors reserved for moving logical information past factories and other congestion.

The bandwidth math is worth one example. In the QROM study (1905.08916), a single empty patch adjacent to a target supports **one CNOT per d cycles** — about 37 kHz at distance 27 with a 1 µs cycle. Make two sides of the target accessible and you double to 74 kHz, near the 100 kHz reaction-limited rate that the classical control loop can feed. Layout is not packing; it is bandwidth engineering. Where the buses run, how wide the hallways are, and how fast the decoder reacts are first-class design constraints — which is exactly why the next lesson's diagrams, and eventually the compilers of tier 3's finale, exist.

## Completing the gate set

Parity measurements alone are not quite a computer — they are Clifford operations, and Clifford circuits are classically simulable (tier 4's tools lean hard on that fact). Two more ingredients close the set:

- **Hadamard.** Apply H transversally to every data qubit, then rotate the patch 90 degrees (an expand-and-contract deformation) to put the boundaries back where the lattice needs them. Cheap, but it shows again that geometry is part of the instruction.
- **Magic-state injection.** Prepare one physical qubit in a special state, grow a small distance-3 patch around it, then merge it up to full distance. This smuggles a non-Clifford resource into the code — the raw material for T gates — and it is noisy, which is why tier 4 exists: cleaning injected states is the job of distillation factories and cultivation.

With merge/split, Hadamard, and injection, the machine is universal. It is worth pausing on how strange that is: a complete quantum computer whose only moving part is a measurement schedule.

## The new instruction set

Collect what you now own. A surface-code computer speaks exactly one native instruction: **joint Pauli parity measurement, by merge**, plus its inverse, the split. Everything else — CNOT, Hadamard, S, T, teleportation — is compiled down into sequences of these, magic-state injection, and classical bookkeeping. This is a profound simplification: hardware only has to do memory and welding; all algorithmic structure lives in the *schedule* of welds.

Which raises the design question the rest of tier 3 answers: how do you *draw* a schedule of merges and splits, check it is correct, and cost it? Answer: in one more dimension. Time to go 3D.

## Try it

The widget below shows the surgery preset: **two distance-3 patches** (9 data qubits each) parked with a gap between them — two chessboards waiting to be welded.

- Press **Merge patches**. For about 1.2 seconds you watch new check faces appear in the gap — the seam stabilizers being switched on — then a banner declares the patches merged.
- Press **Split** and the seam faces vanish: the boards are cut apart again.
- You can still click data qubits to place X or Z errors on either patch and watch checks fire, exactly as in the tier 1 playground; the counter tracks both patches at once.

Now the honesty, and it matters: **this widget is an illustration, not a simulation.** Nothing here performs a real joint parity measurement — no intermediate qubits are initialized, no seam syndromes are extracted for d rounds, no logical parity is computed or tracked in a Pauli frame. The gap faces are drawn to give you the *picture* of welding, and the banner's claim ("the two patches now act as one logical qubit pair") is narrated, not derived. The real merge's content — that the product of seam checks *equals* X_L X_L — is a stabilizer-algebra fact you would verify on paper or with the tier 5 tools, not something a sketch can show. Treat the widget as the cartoon on the whiteboard, and the paragraphs above as the physics.

## Key numbers

- Rough merge = measurement of **X_L X_L**; smooth merge = **Z_L Z_L**; every merge/split costs **d rounds** of syndrome extraction and preserves distance (arXiv:1111.4022).
- Merge outcomes are never corrected physically — they live in the classical **Pauli frame**.
- Split of a|0>+b|1> gives **a|00>+b|11>** (free Bell pairs); safe splitting starts from a **d x 2d** patch.
- CNOT = merge + split + merge around a |+>_L ancilla; with the d x 2d trick it costs **d rounds** total. General multi-body Pauli measurement is the native gate (1808.06709).
- Rotated patch: **d² data + d² − 1 measure** qubits; distance-3 example: 25 unrotated → **13 rotated**.
- Distance-3 logical CNOT, 2D nearest-neighbor: **143** qubits braided → 104 → **53** with surgery on rotated patches.
- Tiled storage at scale: **3d² physical per logical** vs ~12.5d² double-defect (1808.06709/1905.08916).

## Next

Merges and splits happen in time — draw every weld of a whole algorithm in 2D + time and a 3D plumbing diagram appears. That diagram is the compilation target of the entire field: [space-time plumbing and blockgraphs](#/lesson/spacetime-blockgraphs).
