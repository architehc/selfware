# The surface code as a chessboard

This is the central lesson of the lab. Everything after it — decoding, surgery, factories, compilers — is a variation on one object: the **surface code**. The good news is that the whole idea fits on a chessboard.

## The chessboard

Picture a chessboard. The squares come in two colors, and we are going to give each color exactly one job:

- The **white squares** hold the pieces. Each white square stores a little fragment of quantum information. These are the **data qubits**.
- The **black squares** hold the referees. Each black square watches the white squares around it and raises a flag when something is off. These are the **measure qubits** (also called ancilla qubits), and each one performs a **check**.

That is the entire architecture. A surface-code chip is a chessboard: pieces and referees, alternating, forever. The quantum information lives on the white squares; the protection lives on the black squares.

In the interactive widget at the bottom of this page, the drawing convention is slightly different but means the same thing: data qubits are drawn as **dots**, and the checks are drawn as the **colored squares** between them. Dots are the pieces; colored squares are the referees.

## Two colors of referee

There are two kinds of check, and they come in two colors in the widget:

- **Z-checks** (blue squares) catch **X errors**.
- **X-checks** (orange squares) catch **Z errors**.

We will define X and Z errors properly in the next lesson. For now, an **X error** is a bit-flip — the quantum version of a 0 becoming a 1 — and a **Z error** is a phase-flip, a second, genuinely quantum kind of error with no classical analog. The crossed pairing (X errors trip Z-checks, Z errors trip X-checks) is a rule of the physics, not a design choice, and it is worth memorizing now: **the check that catches an error is always the opposite type**.

A referee never looks at a single piece. Each check measures a *relationship* among the (up to) four data qubits touching its square: roughly, "do your neighbors still agree with you?" Because it only ever compares, it learns nothing about the actual stored data — which is exactly how the code dodges the rule that measuring a quantum state destroys it.

## What a check reports

Each check outputs one bit per round: **quiet** (nothing wrong) or **fired** (something changed). In the widget, a fired check lights up with a bright outline.

Two behaviors make the system work:

- **Parity.** A check does not count errors; it reports whether an *odd or even* number of its data qubits were hit. One error on a neighbor fires it; a second error on another neighbor turns it back off. Fired means "odd", not "how many".
- **Locality.** Each referee sees only its own square. No check ever learns the global state — and therefore no error that only affects a few squares can hide from *all* referees. Something local always tattles.

## The rotated patch

Real chips do not use an infinite chessboard; they use a finite patch with a clever, space-saving arrangement called the **rotated** surface code (the board is tilted 45 degrees, which lets the same protection fit in fewer qubits).

The widget shows a distance-3 rotated patch. Count the pieces and referees:

- **9 data qubits** (dots) in a 3 × 3 arrangement. In general a distance-d patch has **d²** data qubits.
- **8 checks** (colored squares): 4 full squares inside the patch, each touching 4 data qubits, plus 4 **half-checks** on the rim, each touching only 2. In general: **d² − 1** checks.

Those 9 + 8 = 17 qubits together store exactly **one logical qubit**. The half-checks on the rim are not a bug or a clipped drawing — they are how a finite patch ends, and the next two lessons will show that the *way* a patch ends is one of the deepest ideas in the field. Note the pattern already: blue half-checks sit on the top and bottom edges, orange ones on the left and right.

The distance-3 patch, drawn flat (q = data qubit, [X] / [Z] = full checks, (x) / (z) = rim half-checks):

```
        (z)
   q    q    q
     [X] [Z] (x)
   q    q    q
 (x) [Z] [X]
   q    q    q
        (z)
```

Count them: 9 data qubits, 4 full checks, 4 half-checks — and notice the half-checks sit in a staggered pinwheel: blue Z halves on the top and bottom edges, orange X halves on the left and right. That stagger is not decoration; it decides which edge absorbs which error type, and the boundaries lesson will make it load-bearing.

## Why "rotated"?

The original surface code draws the chessboard untilted, and it works — but it spends about twice as many qubits for the same distance. The rotated layout is the same code tilted 45 degrees, and the tilt lets the rim half-checks close the patch neatly: same distance, same protection, roughly half the data qubits. Since qubits are the scarcest resource in the field, everyone uses the rotated version, and so does this lab. When you read "d² data qubits" anywhere in this lab, that is the rotated count.

## One round in the life of the chip

A real surface-code chip does not measure each check once; it measures all of them, over and over, forever. On superconducting hardware one full round of check measurements takes about a microsecond, so the referees blow their whistles a million times per second, error or no error.

This repetition is not optional. The checks themselves are physical measurements and can lie — a referee can whistle at nothing or sleep through a hit. Watching each check *over time* and trusting only changes is what turns a stream of unreliable measurements into reliable information. That is the time dimension, and it is the first topic of tier 2. For this lesson, the static snapshot in the widget is the right picture: one round, some errors, some flags.

## What the widget simplifies

Honesty about the cartoon, so the papers do not surprise you later:

- The widget draws one dot per data qubit and folds each measure qubit into its colored square. On a real chip every check square is another physical qubit wired to its four neighbors.
- Errors in the widget appear when you click. Real errors arrive from noise, at random, every round.
- The widget knows exactly which errors you placed; a real chip only ever sees the fired checks and must *guess* the errors. That guess is decoding — tier 2.

The physics the widget *does* show — which check fires for which error, parity cancellation, half-checks on the rim — is exact, and you can verify every claim in these lessons against it.

## Why topology protects

Here is the payoff, and it is worth reading twice.

The logical qubit's information is not stored in any one data qubit, or any small group of them. It is stored in a **global pattern** spanning the whole patch — in how all the qubits relate to each other, not in what any of them individually holds.

Now suppose noise damages a few data qubits. That is **local damage**: it touches a few squares. But the information is a global pattern, and local damage cannot rewrite a global pattern — for the same reason that keying one car in a parking lot does not rearrange the parking lot. What local damage *does* do is trip the local referees, and the referees' flags tell the classical control computer exactly where to repair.

"Topological" is the mathematician's word for this trick: properties that live in the overall shape of a thing, and therefore survive any small, local deformation. The surface code stores its data in a topological property. Local noise can scratch it, trip it, annoy it — but not change it, as long as the scratches stay small and get repaired.

There is a catch, previewed in the first lesson: the repairs must outpace the damage. If errors strike faster than the checks can catch and fix them — if the hardware sits above the **threshold** error rate — the scratches link up into a gash across the whole patch, and *that* does rewrite the pattern. The third lesson of this tier makes "gash across the whole patch" precise: it is called a logical error, and its minimum length is the code **distance**.

## Why exactly one logical qubit?

The counting is worth seeing once, because it explains a lot of later architecture. A distance-d rotated patch has d² data qubits and d² − 1 checks. Each independent check removes one degree of freedom from the data qubits' enormous joint state space, and the arithmetic comes out to exactly **one** remaining degree of freedom: one logical qubit per patch.

That is why surface-code architectures are always drawn as many patches tiled together, with corridors between them: one patch, one logical qubit, no exceptions. Wiring a computer out of surface codes means wiring patches to each other — which is what boundaries are for, and where the last lesson of this tier goes next.

## Try it

Play with the widget below — click to inject errors, watch checks fire. Concretely:

- With the **X error** button selected (it is the default), click the center data qubit. The two blue Z-checks touching it light up, and the counter line reads *1 data-qubit errors, 2 checks fired*. Remember the crossing rule: X errors trip Z-checks.
- Click a neighboring data qubit. The check the two errors share turns *off* — two errors on one check is even, and the check only reports odd. The flags have moved to the ends of your little chain; the next lesson explains why that is the most important behavior in the whole field.
- Switch to **Z error** and click around: now the orange X-checks light up instead. Same physics, opposite channel.
- Click a corner data qubit with an X error: only *one* check fires. The other end of the damage ran off the edge of the patch. Hold that thought for the boundaries lesson.
- Use **erase** to clean up, and watch the counter return to *0 data-qubit errors, 0 checks fired*.

## Key numbers

- A distance-d rotated patch: **d² data qubits**, **d² − 1 checks**, **1 logical qubit**.
- The distance-3 widget: 9 data qubits, 8 checks, 17 qubits total.
- Each interior check watches 4 data qubits; each rim half-check watches 2.
- One error fires (at most) the 2 opposite-type checks touching that qubit; two errors on one check cancel (parity).
- The crossing rule: X errors fire Z-checks; Z errors fire X-checks.

## Next

Why do the flags always appear in pairs? Because errors create *anyons*: [stabilizers and anyons](#/lesson/stabilizers-anyons).
