# Minimum-weight perfect matching

The decoder's input is a scatter of detection events in spacetime; its job is to guess the error chains that have exactly those endpoints. This lesson builds the machine that does the guessing. The machine is disarmingly simple to state — *pair up the events so that the total length of the guessed chains is as small as possible* — and it has a name: **minimum-weight perfect matching**, MWPM for short. Nearly every surface-code decoder ever run on hardware is this idea, dressed up.

## Pair the anyons

Start with the picture. The fired detectors are points; every pair of points could be the two ends of one error chain. A **matching** is a choice of pairs: every event gets exactly one partner. (If the event count is odd, the leftover events each pair with the nearest **boundary** — the edges absorb single endpoints, as you know from tier 1. The boundary acts as a wildcard partner that is always available.)

Each potential pair carries a **weight**: the length of the shortest chain that could connect the two events. On a plain grid that is just the **Manhattan distance** — steps horizontally plus steps vertically, the way a taxi crosses a city grid, with diagonal shortcuts forbidden. Pair two events three steps apart and you are guessing a 3-error chain.

A word on the name. In graph theory a matching is **perfect** when it covers every vertex — nobody left unpaired — and that is exactly the decoding requirement, since an unexplained detection event is an unreported crime. The boundary wildcard is what makes perfection always attainable: with it standing by, an odd number of events still pairs up completely, because the leftovers take the boundary as partner. Hence *minimum-weight perfect matching*: perfect = everyone paired, minimum-weight = shortest total story.

The **total weight** of a matching is the sum of its pair weights: the total length of all the chains you are guessing. And the decoding rule is:

> Choose the matching of minimum total weight. Guess the shortest chains that explain the evidence.

That is the whole algorithm. Everything else is why it works and how to compute it fast.

## A matching by hand

Before the theory, feel the mechanics on the smallest nontrivial case — the same four events the widget below will show you, parked at the corners of a square with side length 3 (so every side-to-side pair has Manhattan distance 3, every diagonal pair 6):

```
   2 . . . 3        the four defects (events), numbered 0-3:
   . . . .          0 = (1,1)   1 = (4,1)
   . . . .          2 = (1,4)   3 = (4,4)
   0 . . . 1
```

(The coordinates are read off the widget's grid; y grows upward in the picture, downward in the widget, which changes nothing.) Three ways to pair four points exist, and only three:

- **Horizontal pairs** {0–1, 2–3}: weight 3 + 3 = **6**.
- **Vertical pairs** {0–2, 1–3}: weight 3 + 3 = **6**.
- **Diagonal pairs** {0–3, 1–2}: weight 6 + 6 = **12**.

Minimum weight: 6, achieved twice. The decoder returns either side-pairing; the diagonal story — four times as much guessed error — is rejected, exactly as "shorter chains are likelier chains" demands. Notice that nothing in this procedure looked at the errors themselves: the *entire* input was four endpoint positions. That is the defining feature of syndrome-based decoding, and its vulnerability too — the decoder can be fooled precisely when nature's true chains happen to pair the endpoints differently than the shortest guess does, in a way that spans the patch.

## Why minimum weight ≈ most probable

The rule smells like a heuristic, but it is almost a theorem. Assume each link of the graph errors independently with probability p (and p is small). A chain of length l then occurs with probability about p^l — every extra link multiplies the probability by another factor of p. At p = 0.001, a 3-link chain is a thousand times likelier than a 4-link chain explaining the same two endpoints.

So among all the stories that produce the same detection events, shorter chains dominate, and the probability of a whole matching decays exponentially with its total weight. Run the numbers on the hand-worked square above, at p = 0.001 per link: a side pairing guesses two 3-link chains (probability ~p⁶ = 10⁻¹⁸, up to counting of equivalent paths), while the diagonal pairing guesses two 6-link chains (~p¹² = 10⁻³⁶). The minimum-weight story is a million million million times likelier. Minimizing total weight is (approximately) maximizing total probability:

- "Approximately" because shortest is not *identical* to most probable — many medium-length paths can collectively outvote one short path. The next lesson makes the probability accounting exact with proper weights.
- But the approximation is excellent, and a landmark result says exactly how excellent — keep reading.

## Blossom: the polynomial miracle

A naive way to find the minimum matching is to list all matchings and weigh each. That way lies madness: the number of perfect matchings of n events is the double factorial (n−1)!!, which grows faster than exponentially. Four events have 3 matchings; six have 15; a hundred have about 10^78 — comparable to the number of atoms in the observable universe. Brute force is dead on arrival for any real experiment, which streams thousands of events per second.

The rescue is one of the classics of computer science: Jack Edmonds' **blossom algorithm** (1965) finds the exact minimum-weight perfect matching in *polynomial* time — roughly O(n⁴) in its original form, improved over the decades to O(n³) and better. Polynomial, not factorial: a hundred events is a rounding error, and surface-code-specific versions do far better still, because below threshold the events cluster into small, independent local groups and the matching decomposes. (Later lessons meet the streaming, parallel descendants; the point here is that an *efficient exact algorithm exists*, and without it the whole architecture would be a paper fantasy.)

You do not need blossom's internals to use a decoder, but you should know its shape: it grows "exploration regions" around unmatched events, and when two regions collide along a tight edge, that edge joins the matching; odd cycles of tentative edges ("blossoms," hence the name) are shrunk and handled as single units. The output is guaranteed-minimum, not approximate. When you meet PyMatching or Google's Sparse Blossom in the wild, they are this algorithm, specialized to spacetime graphs.

## What minimum weight cannot see

Worth being honest about the limits, because the next lesson's upgrades only make sense against them. Minimum *length* decoding assumes three things that real hardware violates:

- **Uniformity**: every link equally likely. Real gates have measured, wildly different error rates — a CNOT is not an idle qubit.
- **Independence**: X and Z errors decoded as strangers. Real noise correlates them (a Y error is both at once).
- **One story**: MWPM returns the single most probable set of chains, but the *optimal* decoder asks a subtly different question — which *class* of corrections (differing by closed loops) is most probable when you sum all the stories in each class. Summing over stories is exponentially hard in general; matching's single-story shortcut is why it is cheap, and the ~0.5% threshold gap you are about to meet is the price of that shortcut.

The first two limits are fixable with better weights and a second pass — the next lesson. The third is fundamental to matching and is why "optimal" and "matching" thresholds are quoted as different numbers.

## From picture to pipeline

Before the thresholds, place MWPM in the machine it serves. A running surface-code computer executes this loop, once per round, forever:

- **Measure** all checks; compare with last round to get detection events (the previous lesson's machinery).
- **Stream** the events into the decoder's spacetime graph — append, never rebuild; the decoder keeps a sliding window of recent history, not the whole past.
- **Match** the unmatched events: blossom on the weighted graph, pairing events with each other or the boundary.
- **Track** the implied correction in software. Corrections are almost never applied to the qubits physically; they are accumulated in a *Pauli frame* — a running note of "the logical qubit is currently flipped in these ways" — that later measurement results are interpreted against. Applying physical corrections would just add more noisy gates.

That last point surprises everyone once: error *correction* on a quantum computer is mostly bookkeeping. The qubits keep their errors; the classical computer keeps the score. Physical intervention happens only when the score says a logical readout must be reinterpreted — and keeping that score correctly, at a million rounds per second, is why decoding is an engineering discipline and not a footnote.

## The threshold is a phase transition

Why trust this cheap rule at all? Because of the deepest result in the field. In 2001, Dennis, Kitaev, Landahl, and Preskill ([quant-ph/0110143](https://arxiv.org/abs/quant-ph/0110143)) mapped the decoding problem onto a model from statistical mechanics — a disordered magnet — and proved that the surface code's **accuracy threshold is a phase transition** of that model:

- Below the threshold error rate, the likely errors form small, separated clusters; minimum-weight guessing repairs them, and enlarging the code helps exponentially.
- Above threshold, error chains percolate across the whole patch no matter what you do; bigger codes fail faster. Order versus disorder, decided at one sharp critical point — that is why "the threshold" is a number and not a mood.

The numbers, in increasing realism:

- With **perfect check measurements**, the critical point sits at p ≈ **10.9%** (the Nishimori point of the 2-D random-bond Ising model, pinned down to 10.94 ± 0.02%). This is the absolute best any decoder could do with ideal referees.
- **MWPM itself achieves ≈ 10.25%** in that same idealized setting — only half a percent below the optimal threshold. A polynomial-time, almost-greedy algorithm buys nearly all the protection the physics offers. That gap is the price of cheap decoding, and it is small.
- With **noisy measurements** (the detectors of the previous lesson), the provable bound drops but stays healthy: p ≥ 1.14% rigorously, with simulations higher.
- And for the full circuit-level model — every gate, every measurement faulty — Fowler later proved that matching decoding has a genuine finite threshold at p ≥ **7.4 × 10⁻⁴** ([arXiv:1206.0800](https://arxiv.org/abs/1206.0800)). A proof, not a fit: below that error rate, arbitrarily reliable quantum computation with matching decoders is guaranteed.

## Ties are (usually) harmless

One subtlety the widget below will show you live: sometimes two matchings tie for minimum weight. Panic not. When two equally short explanations differ, they typically differ by a **closed loop** of flips — and a closed loop is a product of stabilizers, which acts on the logical qubit as the identity. Either correction works; the tie is between two correct answers. (The dangerous case is a tie between matchings that differ by a *spanning* chain — but that requires an error pattern already halfway to a logical operator, which is the rare, heavy failure mode the distance suppresses.)

## Try it

The widget below is the matching game played in slow motion, with four defects (fired detectors) parked at the corners of a square: lattice points (1,1), (4,1), (1,4), and (4,4), numbered 0–3. All edge weights are plain Manhattan distances.

- There are exactly **3 candidate matchings**. Press **Step** to walk through them in enumeration order. The status line shows each candidate's index and total weight, plus the best weight seen so far.
- The first two candidates each pair neighbors along sides of the square — horizontal pairs, then vertical pairs — at total weight **6** (two edges of 3 each). Watch the coloring: the current candidate is drawn in **blue**, the best-so-far in **green**. Candidate 1 is immediately the champion and shows green; candidate 2 ties it at 6 but a tie does not dethrone the incumbent, so it shows blue against the green best.
- The third candidate pairs along the **diagonals**: two chains of Manhattan length 6 each, total weight **12** — the long-chain story, twice the price, correctly rejected.
- On the last step the widget declares the verdict: *minimum weight = 6 — this is the decoder's correction*. Either of the tied weight-6 matchings is a valid answer; they differ by a loop around the square, a stabilizer, so the choice between them is free.
- **Reset** restarts the walk.

One honest disclaimer, true for every widget in this lab: this stepper finds the minimum by **brute force** — it enumerates all 3 matchings and weighs each. That is a teaching simplification. Real decoders never enumerate; they run the **blossom algorithm**, which finds the same answer in polynomial time even with millions of events. With at most a handful of defects, brute force is instant and shows you exactly what "minimum weight" means — which is all we ask of it.

## Key numbers

- MWPM: pair all detection events (boundary as wildcard) to minimize total chain length ≈ maximize probability, since a length-l chain costs ~p^l.
- Matchings of n events: (n−1)!! — 3 for 4 events, 15 for 6, ~10^78 for 100. Brute force impossible; Edmonds' **blossom algorithm** (1965) solves MWPM exactly in polynomial time.
- Threshold is a stat-mech phase transition (DKLP 2001): perfect-measurement critical point **≈ 10.9%**; plain MWPM reaches **≈ 10.25%** — cheap decoding within ~0.5% of optimal.
- Noisy-syndrome rigorous bound p ≥ 1.14%; circuit-level finite threshold for matching proved at p ≥ **7.4 × 10⁻⁴** (arXiv:1206.0800).
- The widget: 4 defects, 3 candidates, weights 6, 6, 12 — minimum 6, a harmless tie broken by arrival order.

## Next

Minimum weight is minimum *length* — it assumes every link is equally likely, and it decodes X and Z separately. Real chips know better: some gates are flakier than others, and a Y error talks to both channels at once. Feeding that knowledge into the weights is worth a fortune — [weighted and correlated decoding](#/lesson/weighted-correlated).
