# Weighted and correlated decoding

The MWPM decoder of the last lesson weighs every chain by its length — a ruler that assumes every qubit, gate, and measurement is equally flaky. Real chips know better: a two-qubit gate might fail ten times as often as an idle qubit, and a single physical hiccup can fire detectors in *both* the X and Z channels at once. This lesson is about feeding that knowledge into the matching. The code does not change at all — only the decoder gets smarter — and the payoff is the most famous threshold number in the field, plus an almost-free factor of two in error suppression.

## Distance is a blunt ruler

Plain Manhattan distance says: every link of the spacetime graph is equally likely to host an error. But the links are not born equal. Each link corresponds to concrete physical events — this CNOT gate failing, that measurement lying, this qubit decaying while idling — and those events have known, measured, different probabilities. Weighing them all alike throws away information the hardware engineers worked hard to calibrate.

Worse, the naive decoder actually runs *two independent matchings*: one on the X-error graph (e anyons), one on the Z-error graph (m anyons). But depolarizing noise — the standard model of real noise — produces **Y errors**: an X and a Z on the same qubit at once, firing both graphs *at the same place*. Treating the two graphs as strangers discards a correlation that is sitting there in the data, waving.

Both fixes are cheap, and both are just reweighting.

## Weights from −ln p

Here is the exact version of "short chains are likely chains." If a physical fault mechanism fires a given detector pair with probability P, set that edge's weight to

> w = −ln P

The minus-logarithm is not decoration; it is the trick that makes addition mean multiplication. The probability of a whole chain is the *product* of its links' probabilities — and products of probabilities become *sums* of −ln p weights. So the total weight of a matching **is** (the negative log of) its probability, and minimum-weight matching becomes *most-probable-error* matching, exactly, given the noise model.

Wang, Fowler, and Hollenberg ([arXiv:1009.3686](https://arxiv.org/abs/1009.3686)) did this seriously: they enumerated every way every gate in the syndrome-extraction circuit can fail, grouped the faults by which detector pair they fire, summed the probabilities, and turned each into a weight. Two findings matter for you:

- The simplest approximation — the single most probable fault path per detector pair — already captures essentially all the gain. Fancier versions that sum over all connecting paths shift the weights by only 10–20% on links that were not the leading contribution anyway, and do not move the threshold. *Calibrate coarsely, match exactly* is good enough.
- The decoder, not the code, is where threshold lives. Nothing physical changed; only the ruler did.

## A worked weight

Make the −ln p arithmetic concrete. Suppose calibration says a particular CNOT gate in the check circuit fails with probability 10⁻³, and when it fails in a particular way it fires exactly detectors A and B. Then the A–B edge gets weight

> w = −ln(10⁻³) ≈ 6.9

Now compare two explanations of a fired A–B pair. The one-hop story (that single CNOT fault) costs 6.9. The two-hop story — two independent faults on a path through an intermediate detector, each with probability around 10⁻³ — costs 6.9 + 6.9 = 13.8. The decoder prefers the short story by 6.9 weight units, which is exactly a probability ratio of e^6.9 ≈ 1000: one thousand to one, matching the raw odds of one fault versus two. The weights are log-odds; adding them is multiplying odds. That is the entire content of "weighted MWPM" — and also why plain Manhattan distance was already a decent approximation: at uniform p, −ln p is the same for every link, so weight is just length times a constant.

Real calibration goes further in two directions: down to the device, where weights are built from *measured* detection-event statistics rather than an assumed model, and out to the full circuit, where every gate, idle, and measurement contributes its own fault paths. The detector graph with these weights is exactly the "detector error model" format that modern tools (Stim, PyMatching — tier 5's hands-on stack) pass between simulator and decoder.

## The 1% threshold

The payoff, from that same 2010 paper: the circuit-level threshold jumped from the previous **0.75%** to

- **1.1%** under the standard depolarizing noise model,
- **1.2%** under a balanced model,
- **1.4%** under an ion-trap-style model where measurement and idle errors are rarer than gate errors.

This was the first time a geometrically constrained, 2-D, nearest-neighbor quantum architecture crossed the 1% mark — the number that made experimentalists sit up, and the number the "is your hardware below threshold?" question is usually asked against. Note also the spread: 1.1% versus 1.4% is the same code, the same decoder, different *noise calibration*. Thresholds are properties of code + decoder + noise model together; anyone quoting you a threshold without naming all three is selling something.

One honesty footnote you should carry as a reader of papers. The 1.1% figure came from fits to small codes (distance up to ~13). Later work pushing to distance 55 found the true crossing at **0.9%** — boundary effects distort small-distance curves and inflate the extrapolation ([arXiv:1110.5133](https://arxiv.org/abs/1110.5133)). Practically the difference is moot (real operating points sit near 0.5% and below), but as a lesson in reading threshold claims it is priceless: *ask how large the simulated distances were.*

## Correlated decoding: let X and Z talk

Second upgrade. A Y error fires the X-graph and the Z-graph at the same location. So once one matching is confident about its half, the other half's odds change dramatically: under depolarizing noise, given that an X error almost certainly happened at a spot, the conditional probability of a Z error there is about **1/2** — vastly higher than the background rate. Fowler's two-pass scheme ([arXiv:1310.0863](https://arxiv.org/abs/1310.0863)) exploits exactly this:

- Decode one graph (say Z) as usual.
- Wherever that matching confidently places an error, **reweight** the corresponding edge in the X graph to w = −ln(1/2) — a cheap edge, inviting the second matching to use it.
- Re-match the X graph with the new weights.

The gains, and they are large:

- In the idealized setting, correlated decoding lowers the logical error rate by a factor of about **2^(d/2)** at distance d — at d = 20 that is a factor of a thousand, information-theoretically optimal; no decoder can do better there.
- At the full circuit level, the number to memorize: at physical error rate p = 10⁻⁴, the error-suppression ratio between distance 3 and distance 5 improves from **95 to 188** — roughly a **factor 2 more suppression per two units of distance**. That is equivalent to gaining about one full distance step of protection *for free*: same patch, same qubits, smarter decoder.
- The cost is a second matching pass — about **2× the compute** — and the reweighting only disturbs matches near the reweighted edges, so the streaming structure survives intact. Two times the work for two times the suppression per size-up is a trade you take every day.

## Parallel and streaming: decoding at clock speed

Everything so far sounds batch — gather events, match, correct — but a real machine streams syndromes every microsecond, forever. The third pillar of this lesson is that matching decoders were re-engineered for exactly that, in a line of work crowned by [arXiv:1307.1740](https://arxiv.org/abs/1307.1740):

- Below threshold, error clusters are small and local, so matching decomposes into independent local problems. The proof: with a 2-D array of simple processing elements, each owning a fixed patch of the lattice and talking only to its neighbors, the average classical processing time per round is **O(1) — independent of the code size**. Decoding a million-qubit machine takes the same average time per round as decoding a toy.
- Memory stays bounded: the probability of needing syndrome data from far in the past decays exponentially with distance into the past, so a fixed-multiple lookback buffer suffices, and the decoder never re-solves history from scratch.
- Rare hard instances (a big, nasty cluster) make one processor fall behind linearly with exponentially small probability; designing in 2× speed headroom lets it catch up while its neighbors sail on. Bursts stall locally, never globally.

This is the license for real-time decoding, and it foreshadows tier 4: in 2024 Google ran a correlation-augmented, parallelized blossom decoder *live* against a superconducting chip cycling every 1.1 µs, sustaining throughput with an average latency of **63 µs at distance 5, constant over a million rounds** ([arXiv:2408.13687](https://arxiv.org/abs/2408.13687) — the below-threshold paper; its 2022 predecessor, arXiv:2207.06431, demonstrated the d = 5-beats-d = 3 scaling but decoded only offline). The path from Edmonds' 1965 chalkboard algorithm to a 63-microsecond adjunct of a quantum chip runs straight through the three ideas of this lesson: calibrated weights, correlation passes, and parallel streaming.

## A checklist for decoder claims

Decoder papers quote seductive numbers; tier 4 will have you reading several. Three questions keep you oriented:

- **Accuracy, latency, or throughput?** A decoder can be extremely accurate but slower than the syndrome stream (useless in real time — undecoded data piles up and the backlog grows faster than linearly), or fast but sloppy. Serious papers report all three, against a stated cycle time.
- **Which noise model?** Thresholds and suppression ratios are properties of code + decoder + noise model together; the 1.1%-versus-1.4% spread above is calibration, not physics. "Threshold" with no noise model attached is marketing.
- **How large were the simulations?** Small-distance fits extrapolate badly (the 1.1% → 0.9% revision). Look for runs at distances well beyond the ones fitted.

You will reuse this checklist on every experimental claim in tier 4 — including the 63 µs one.

## What this lesson left out

Matching is not the only decoder family, and honesty requires naming the neighbors you will meet in tier 4 and the papers:

- **Union-find** decoders trade a little accuracy for even simpler, nearly-linear-time operation — a favorite for hardware (FPGA) implementations.
- **Belief-propagation front ends** (belief-matching) feed better probability estimates into the same matching back end, closing much of matching's accuracy gap at similar cost.
- **Neural and tensor-network decoders** can beat matching's accuracy outright, but so far at speeds or scalings that keep them offline — the 2024 real-time demonstration used correlation-augmented blossom precisely because the more accurate alternatives were 20× too slow.

Every one of these still consumes the same input — a detector graph with −ln p weights — and still answers the same question: which chains best explain the endpoints. Weighted, correlated, streaming MWPM is the reference point they are all measured against, which is why this lesson gave it the full treatment.

## Try it

The widget below shows what one reweighted edge does to a matching. Six defects sit on the grid at (1,1), (2,3), (4,1), (1,4), (4,4), and (5,3), numbered 0–5:

```
   3 . . 4 .
   . 1 . . 5
   . . . . .
   . 0 . 2 .
```

All edges carry Manhattan weights **except one**: the edge between defects 0 and 1 has been overridden to **0.5** (plain Manhattan would charge it 3) — a stand-in for calibration shouting "this pair co-fires all the time; a chain between them is far more probable than its length suggests." (In a real decoder that 0.5 would be a −ln P computed from the noise model; here it is a teaching value.)

- Six defects means **15 candidate matchings**. Press **Step** to walk them; as before, the current candidate is **blue**, the best-so-far **green**, and the status line reports both weights.
- Watch for the moment a candidate uses the cheap 0–1 edge. Without the override, the best pairing of this configuration costs **7** (pair 0–2, 1–3, 4–5). With it, the winner takes the 0.5 edge and completes with two pairs of cost 3: total **6.5**. The cheap edge flips the decoder's answer.
- The final step declares the verdict: *minimum weight = 6.5 — this is the decoder's correction*. **Reset** restarts the walk.
- Notice what you just witnessed is the entire mechanism of this lesson: the matching algorithm did not change at all. Only a weight did — and a different correction came out. Weights are how everything the lab knows about noise reaches the decoder.

The same disclaimer as last lesson applies, verbatim: the stepper finds its answer by **brute-force enumeration** of all 15 matchings — a teaching simplification that is instant for six defects and impossible for six thousand. Real decoders run the **blossom algorithm** on the weighted graph and find the identical minimum in polynomial time.

## Key numbers

- Edge weight = **−ln P** per fault mechanism: products of probabilities become sums of weights, so minimum weight *is* most probable.
- Noise-calibrated weighting lifted the circuit-level threshold from **0.75% → 1.1–1.4%** by error model (arXiv:1009.3686) — first 2-D-local threshold over 1%. Later large-distance runs revised the true crossing to **0.9%** (arXiv:1110.5133): beware small-distance extrapolation.
- Correlated X/Z decoding (arXiv:1310.0863): reweight to −ln(1/2) where the other channel is confident; ~**2^(d/2)** lower logical error ideally; circuit-level d = 3 → d = 5 suppression ratio **95 → 188** at p = 10⁻⁴; cost ≈ **2×**.
- Parallel/streaming MWPM (arXiv:1307.1740): **O(1)** average classical time per round, independent of code size, with bounded memory and local-only communication.
- Real-time existence proof: **63 µs** average latency at distance 5 against a 1.1 µs cycle, constant over 10⁶ rounds (arXiv:2408.13687 — not the offline-only 2207.06431). Tier 4 tells that story.
- The widget: 6 defects, 15 candidates; one 0.5-weight edge moves the minimum from 7 to **6.5**.

## Next

You now have a memory that works: checks watched over time, events matched by calibrated weights, X and Z compared, all at streaming speed. But a memory is not a computer. Tier 3 teaches the patches to move, merge, and split — [lattice surgery](#/lesson/lattice-surgery).
