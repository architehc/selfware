# Hands-on 3: Clifft — simulating cultivation

Everything in hands-on 1 worked because Stim simulates *stabilizer* circuits: Clifford gates, Pauli noise, measurements. But the whole point of fault tolerance is running **non-Clifford** gates — the T gate, magic states — and a T gate is precisely what Stim refuses to touch. **Clifft** (Unitary Foundation, arXiv:2604.27058) is the simulator built for exactly this gap: *near-Clifford* circuits — Clifford skeletons with a handful of T gates — that are too magical for Stim and too big for brute-force statevector simulators. Its canonical workload is the magic-state cultivation circuits of tier 4's cultivation lesson.

**Verification status: install and the single-T-gate circuit below were run on macOS (Apple Silicon) on 2026-08-05 with clifft 0.7.0; those outputs are real. The d=3 cultivation example is described from the upstream reproducibility repo (unitaryfoundation/clifft-paper) and was not re-run here — it exceeded this lesson's time budget.**

Prerequisite: the `tools-env` environment from hands-on 0, activated.

## Stage 0: watch Stim refuse

First, convince yourself the gap is real. Try to hand Stim a circuit with one T gate:

```bash
cd tqec
source tools-env/bin/activate
python -c "
import stim
stim.Circuit('H 0\nT 0\nMX 0\n')
"
```

**You should see** (real output, 2026-08-05):

```
ValueError: Gate not found: 'T'
```

This is not a missing feature to be patched; it is the design boundary. Stim's speed comes from tracking only stabilizer information, and a T gate takes the state outside the stabilizer world. For cultivation circuits — which inject and distill magic — that boundary is a wall.

## Stage 1: install Clifft

```bash
pip install clifft
```

**You should see:**

```
Successfully installed clifft-0.7.0
```

**If it fails:** Clifft ships a compiled C++ core as a prebuilt wheel; if your platform has no wheel, pip will try (and possibly fail) to build from source — check the repo (github.com/unitaryfoundation/clifft) for the supported-platform list before debugging compilers.

## Stage 2: run the circuit Stim rejected

The idea in one paragraph before the code. Clifft factors the quantum state into three pieces: an **offline Clifford frame** — all the Clifford structure, resolved at compile time, generalizing Stim's compile-once/sample-many trick to magical circuits; an **online Pauli frame** — the same bookkeeping trick tier 4 met in decoding; and a small **active statevector** holding only the degrees of freedom that are *currently* non-Clifford entangled. The exponential cost scales with the peak size of that active state (called `peak_rank` in the API), not with the qubit count — which is why a cultivation circuit with a few T gates is cheap and a random deep circuit is not.

Run the one-qubit circuit Stim rejected:

```bash
python -c "
import clifft, numpy as np
prog = clifft.compile('H 0\nT 0\nMX 0\n')
res = clifft.sample(prog, shots=10000)
bits = np.asarray(res.measurements)
print('shots:', bits.size)
print('P(X-measurement = 1):', round(float(bits.reshape(-1).mean()), 4))
print('theory: sin^2(pi/8) =', round(np.sin(np.pi/8)**2, 4))
"
```

**You should see** (real output, 2026-08-05; sampling noise moves the middle line slightly run to run):

```
shots: 10000
P(X-measurement = 1): 0.1523
theory: sin^2(pi/8) = 0.1464
```

Check the physics, because this is the whole lesson in one number: H puts the qubit on the equator of the Bloch sphere, T rotates it 45° around the Z axis, and measuring in the X basis then yields 1 with probability sin²(π/8) ≈ 0.146. Stim cannot produce that number — the state after T is not a stabilizer state — and Clifft reproduces it exactly (up to shot noise). Note also that the circuit text is Stim-format: Clifft deliberately speaks the same circuit language, so tqec's output extended with T-gate injections is already valid input.

**If it fails:** `compile()` takes the circuit *text*, not a `clifft.Circuit` object — passing `clifft.parse(...)`'s output raises a `TypeError` about incompatible function arguments. And measure in the right basis: H then T then a *Z-basis* measurement (`M 0`) still gives 50/50, because the T phase is invisible until you rotate into X or Y. If your result is 0.5, you wrote `M` instead of `MX`.

## Stage 3: the cultivation example (from the upstream artifacts)

The headline Clifft benchmark is the distance-3 magic-state cultivation circuit from the cultivation paper — the escape stage included — which the Clifft authors simulated exactly, end to end, over hundreds of billions of shots, quantifying the gap between true-T-gate injection and the cheaper S-proxy trick. The reproducibility artifacts live in the companion repo:

```bash
git clone https://github.com/unitaryfoundation/clifft-paper.git
ls clifft-paper
```

**You should see** the paper's circuit files (Stim format, T gates included) and run scripts; the README maps each figure to a script. The d=3 cultivation run goes through the same two calls you just used — `clifft.compile(circuit_text)` then `clifft.sample(prog, shots=...)` — just with a much larger circuit, and with `prog.peak_rank` worth watching: it is the peak active-statevector size, the number that decides whether the run fits in your RAM.

**Honest status:** this stage was not executed during this tutorial's verification pass (time budget); the commands above are from the upstream repos, not run here. Expect the full d=3 cultivation simulation to be a long run even on a good CPU — the paper's speed claims are relative to other near-Clifford simulators (up to ~370× the GPU sampler Tsim on this workload), not to Stim on Clifford-only circuits.

**If it fails:** if `clifft.sample` crawls, print `prog.peak_rank` first — every unit of peak rank doubles the active state, and circuits that accumulate magic faster than measurements release it are exactly the ones Clifft cannot save you from. Postselected runs (cultivation discards failed attempts) route through `sample_survivors` / the `postselection_mask` argument of `compile`; check `help(clifft.compile)`.

## Where this leaves the stack

You have now touched every link of the design-automation chain from the reading list: **TopoLS** turns circuits into blockgraphs, **tqec** turns blockgraphs into Stim circuits, **Stim + PyMatching + sinter** measure logical error rates on the Clifford parts, and **Clifft** covers the magical parts Stim cannot represent — which are, not coincidentally, the parts the whole architecture exists to feed. The tier-4 lessons explained why each piece must exist; this tier proved they exist, run, and agree with theory on your own machine.
