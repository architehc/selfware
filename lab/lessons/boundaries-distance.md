# Boundaries, distance, and logical operators

You have seen that anyons come in pairs — unless one end of the error chain falls off the edge of the patch. This lesson looks hard at those edges. It turns out the *way a patch ends* determines everything: where the logical qubit lives, how strong the protection is, and eventually how patches are wired into a computer.

## Two ways to end a patch

A surface-code patch cannot just stop. Every edge must choose which species of anyon it will **absorb** — which error-chain type is allowed to terminate there, firing no final check. There are exactly two self-consistent choices, so there are exactly two boundary types:

- A **smooth boundary** absorbs X-error chains: an X chain can end on it freely, leaving only a single e anyon behind (or none, if the e is absorbed too).
- A **rough boundary** absorbs Z-error chains, the same story with m anyons.

Look at the rim of the widget's patch and you can read the types off directly. The **left and right edges** carry the orange X-type half-checks; an X error chain reaching those edges stops firing Z-checks, so left and right are the **smooth** boundaries. The **top and bottom edges** carry blue Z-type half-checks and absorb Z chains; those are the **rough** boundaries.

Why must boundaries come in these two types at all? Because a boundary is a place where anyons can condense out of the code, and the physics of the 2D plane offers exactly two single-species options: e or m. (The composite e×m is a fermion and cannot condense by itself.) The rim of a patch alternates between the two, which is why a patch always has two edges of each type, on opposite sides.

## Logical operators: strings that span the patch

Recall from the anyon lesson: a chain of errors whose endpoints are both absorbed leaves *no checks fired at all*. It is completely invisible to the referees.

Now take that seriously. Start an X-error chain at the left (smooth) edge and run it straight across the patch to the right (smooth) edge. Both ends are absorbed, no check ever fires — yet you have undeniably acted on the qubits. If the code cannot see this operation, the operation must be acting on the *logical* qubit. It is: this spanning X string is the **logical X operator**, written X_L — it flips the logical qubit's bit the same way a physical X flips a physical one.

Likewise, a Z chain from the top (rough) edge to the bottom (rough) edge is the **logical Z operator**, Z_L.

Two properties make this beautiful rather than confusing:

- **The two strings must cross.** The horizontal X_L and the vertical Z_L share exactly one data qubit, where they anticommute — which is precisely the rule logical X and Z must obey. The geometry enforces the algebra.
- **The exact path does not matter.** Wiggling the string — bulging it around a check — multiplies it by a stabilizer, and stabilizers act trivially on the logical state. Only the *topology* of the string matters: which two boundaries it connects. This is the master principle of the whole subject: logical operators are not chains but *classes* of chains, and the code cannot tell representatives of a class apart.

That last point is also the danger. An *error* chain that spans the patch is an undetectable logical operator: the one failure mode the checks are structurally blind to.

## Distance: the shortest undetectable error

The **code distance** d is the length of the shortest possible logical operator — the fewest physical errors that can chain all the way from one same-type boundary to the other.

On the rotated patch the geometry is transparent: the straightest string across is a single row (or column) of data qubits, so **d equals the side length of the patch**. A distance-3 patch has 3 × 3 = 9 data qubits; distance 5 has 25. Distance is literally "how many errors have to conspire in a row to fool the code."

## How many errors can the code fix?

Distance d does not mean the code corrects d errors. It corrects **⌊(d−1)/2⌋** — half the distance, rounded down. So d = 3 corrects 1 error, d = 5 corrects 2, d = 7 corrects 3.

The reasoning is a coin-flip argument. The decoder repairs errors by guessing the *shortest* chain that explains the anyons. Suppose the true damage is a chain of t errors somewhere along a would-be logical string of length d:

- If t < d/2, the true chain is still the shortest explanation, the decoder finds it, and the repair succeeds.
- If t ≥ d/2, the *rest* of the logical string (d − t errors) is now the shorter explanation. The decoder guesses wrong, and its "correction" plus the true error completes a spanning chain: a logical error, undetectable by construction.

Halfway across is the tipping point — hence the factor of a half. Growing the distance pushes the tipping point further out, which is the entire scaling strategy of the field.

## Boundaries are ports

The absorption viewpoint makes boundaries sound passive. They are anything but: a boundary is a **port** — a place where anyons can enter or leave the code — and everything built later in this course is plumbing between ports.

A preview of two constructions you will meet properly in tier 3:

- Punch a hole in the patch and the hole's rim is a small boundary of one type. A pair of same-type holes stores a logical qubit: its logical Z is a loop around one hole, its logical X a string between the two holes. This is the original way surface-code computation was laid out.
- Instead of holes, put two patches side by side and *merge* them along matching boundaries, then split them apart. Merge and split measure joint logical properties — and measuring joint properties is all a quantum computer ever needs to do. This is **lattice surgery**, the modern layout.

Both work because of the rule you just learned: a boundary is defined by which anyon it absorbs. Computation, in this world, is the controlled rearrangement of ports.

(A footnote for later paper-reading: the literature's naming of which boundary is "smooth" versus "rough" flips between conventions. What never flips is the physics — each edge absorbs exactly one anyon species. When in doubt, ask which anyon condenses there.)

The widget's overlay, drawn flat — the logical X string (row of X's, smooth edge to smooth edge) crossing the logical Z string (column of Z's, rough edge to rough edge) on exactly one data qubit:

```
            top edge (rough)
               Z
   ·    ·    Z    ·    ·
   ·    ·    Z    ·    ·
   X    X    X    X    X      logical X, left to right (smooth to smooth)
   ·    ·    Z    ·    ·
   ·    ·    Z    ·    ·
               Z
           bottom edge (rough)
```

Every other representative of the same logical operator is this picture with the string wiggled — same endpoints, same effect. The path is negotiable; the endpoints are not.

## Why not just make the distance enormous?

If bigger d is exponentially better, why not d = 1000 everywhere? Three practical brakes:

- **Qubits cost.** A rotated patch uses d² data qubits (plus as many again for the measure qubits), so distance 25 already spends over a thousand physical qubits on one logical qubit. Distance is bought with the scarcest resource there is.
- **The threshold condition.** The exponential suppression only kicks in below threshold. Above threshold, a bigger code fails *faster* — more components, more errors, same spans. Distance is leverage, and leverage only helps when the hardware is already good enough.
- **Latency and factories.** Bigger patches take longer to decode and to operate on, and many operations (the magic-state factories of tier 4) have their own distance requirements. Choosing distances per patch, per factory, per task is an engineering optimization — one of the first places the design-automation toolchain earns its keep.

## Distance buys exponential suppression

Here is the punchline that justifies the ~1000:1 overhead tax from lesson one. Below the threshold error rate, the logical error rate per round scales roughly as

> ε_d ∝ (p / p_thr)^((d+1)/2)

with p the physical error rate and p_thr the threshold. Every time you add 2 to the distance, the logical error rate is divided by a factor called **Λ** (capital lambda), roughly p_thr / p. If your hardware runs at p ≈ 0.1% against a p_thr ≈ 1% threshold, each two extra rows and columns buy you about a factor of 10 in reliability. Need a trillion times better than one round offers? That is a fixed, computable number of extra qubits — the exponential does the heavy lifting.

This is no longer theory. Google measured it: in 2023 a distance-5 code edged out distance-3 (Λ ≈ 1.04, barely above water), and by 2024 improved components gave a clean Λ = 2.14 across distances 3, 5, and 7, with the distance-7 logical qubit outliving its best physical qubit by 2.4×. You will dissect both experiments in tier 4; for now, file Λ away as "the number that says how much each size-up buys."

## A checklist for distance claims

You will meet distance numbers constantly from here on — in papers, in press releases, in tier 5's tool outputs. Three questions decode any of them:

- **Distance of what?** One patch's distance, a factory's internal distances, and a whole computation's effective distance are different numbers. Good papers state distances per component.
- **Physical or logical error rate?** "Error rate 10⁻³" means something completely different depending on which one is quoted. The whole point of the code is the gap between them.
- **Above or below threshold?** A distance number says nothing without knowing whether the hardware sits below threshold — that is what makes more distance help rather than hurt.

## Try it

The widget below is a **distance-5** patch — 25 data qubits, 24 checks — with a new control: the **show logical operators** checkbox.

- Tick the checkbox. Two dashed lines appear: an orange horizontal line from the left edge to the right edge — the logical X string — and a blue vertical line from top to bottom — the logical Z. Note where they end: X_L connects the smooth boundaries, Z_L the rough ones, and they cross at exactly one data qubit.
- Count the data qubits along either dashed line: 5. That count *is* the distance.
- Now reproduce a logical operator by hand. With the **X error** button selected, click the data qubits along the middle row, one at a time, left to right. Watch the fired checks march with the growing tip of the chain — and when your chain touches both edges, every check goes dark. The counter reads *5 data-qubit errors, 0 checks fired*: an invisible, uncorrectable logical error, built with your own mouse.
- Try 2 errors somewhere in the bulk and note the code shrugs them off (d = 5 corrects ⌊(5−1)/2⌋ = 2). Then appreciate the asymmetry: 2 scattered errors are nothing; 5 in a line are fatal.
- Do the same down the middle column with **Z error** for the Z_L string, then **erase** and put the patch back to sleep.

## Key numbers

- 2 boundary types: **smooth** (absorbs X chains / e anyons) and **rough** (absorbs Z chains / m anyons). Each patch has two of each, on opposite edges.
- Logical operators are strings spanning the patch: X_L between the smooth edges, Z_L between the rough edges; they cross on exactly one data qubit.
- **Distance d = length of the shortest spanning string** = side length of a rotated patch.
- A distance-d code corrects **⌊(d−1)/2⌋** errors: d = 3 → 1, d = 5 → 2, d = 7 → 3.
- The d = 5 widget: 25 data qubits, 24 checks, 49 qubits for 1 logical qubit.
- Below threshold: ε_d ∝ (p / p_thr)^((d+1)/2); each +2 in distance divides the logical error rate by Λ. Measured on hardware: Λ ≈ 1.04 (2023) → Λ = 2.14 (2024).

## Next

The referees whistle, the anyons appear — but who decides which chains to repair? Tier 2 opens with how the checks speak over time: [syndromes, detectors, and the time dimension](#/lesson/syndromes-detectors).
