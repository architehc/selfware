# Hands-on 1: tqec — blockgraph to logical error rate

This lesson runs the entire TQEC pipeline once, end to end, on the smallest honest example: a **logical memory experiment**. You will build a blockgraph out of two cubes, compile it to a noisy Stim circuit at code distance 3, sample a hundred thousand shots, decode them with PyMatching, and count how often the logical qubit dies. Then you will sweep the physical error rate with sinter and produce the logical-vs-physical plot — the same artifact every threshold paper in the field is built around.

**Verification status: every command and script on this page was run on macOS (Apple Silicon) on 2026-08-05 with tqec 0.2.0, stim 1.16.0, pymatching 2.4.0, sinter 1.16.0. All outputs shown are real observed outputs.**

Prerequisite: the `tools-env` environment from hands-on 0, activated (`source tqec/tools-env/bin/activate` from the repo root).

## The pipeline, recalled

Tier 5's compilation lesson gave you the shape; here it is as code steps:

1. Build a `BlockGraph` from cubes and pipes.
2. Find its **correlation surfaces** — the logical observables.
3. `compile_block_graph` substitutes verified plaquette templates into every block.
4. `generate_stim_circuit(k=1, ...)` emits a Stim circuit at distance d = 2k + 1 = 3 with noise, detectors, and observables annotated.
5. Stim samples detector and observable flips; PyMatching decodes; the mismatch rate is the logical error rate.

## Stage 1: write the script

Save this as `tqec/hands-on/memory_experiment.py`:

```python
"""Hands-on 1: blockgraph -> Stim circuit -> PyMatching decode -> logical error rate."""
import numpy as np


def main():
    import tqec
    from tqec import BlockGraph, compile_block_graph
    from tqec.utils.position import Position3D
    import pymatching

    # 1. Build a small logical memory experiment: two cubes stacked in time.
    graph = BlockGraph("memory")
    c0 = graph.add_cube(Position3D(0, 0, 0), "ZXZ")
    c1 = graph.add_cube(Position3D(0, 0, 1), "ZXZ")
    graph.add_pipe(c0, c1)
    print("cubes:", graph.num_cubes, "pipes:", graph.num_pipes)

    # 2. Find correlation surfaces (the logical observables).
    surfaces = graph.find_correlation_surfaces()
    print("correlation surfaces:", len(surfaces))

    # 3. Compile to a Stim circuit at k=1 (distance d=3), p=0.001 noise.
    compiled = compile_block_graph(graph, observables=[surfaces[0]])
    noise = tqec.NoiseModel.uniform_depolarizing(0.001)
    circuit = compiled.generate_stim_circuit(k=1, noise_model=noise)
    print("qubits:", circuit.num_qubits)
    print("detectors:", circuit.num_detectors)
    print("observables:", circuit.num_observables)

    # 4. Sample detector + observable flips with Stim.
    shots = 100_000
    sampler = circuit.compile_detector_sampler()
    dets, obs = sampler.sample(shots, separate_observables=True)

    # 5. Decode with PyMatching and count logical errors.
    dem = circuit.detector_error_model(decompose_errors=True)
    matcher = pymatching.Matching.from_detector_error_model(dem)
    predictions = matcher.decode_batch(dets)
    logical_errors = int(np.any(predictions != obs, axis=1).sum())
    print(f"shots: {shots}")
    print(f"logical errors: {logical_errors}")
    print(f"logical error rate: {logical_errors / shots:.2e}")


if __name__ == "__main__":
    main()
```

Read it once before running. The two `ZXZ` cubes are surface-code patches stacked along the time axis; the pipe between them is "keep existing for one more block of rounds". The string `ZXZ` names which basis each face of the cube terminates in — the boundary types from tier 3, written down as data. The `if __name__ == "__main__":` wrapper is *not* optional decoration; see the troubleshooting note below.

## Stage 2: run it

```bash
cd tqec
source tools-env/bin/activate
python hands-on/memory_experiment.py
```

**You should see** (real output, 2026-08-05):

```
cubes: 2 pipes: 1
correlation surfaces: 1
qubits: 25
detectors: 48
observables: 1
shots: 100000
logical errors: 500
logical error rate: 5.00e-03
```

Pause on the middle three lines. The compiler took your two abstract cubes and produced a concrete 25-qubit circuit with 48 detectors — the syndrome checks of tier 3, automatically placed — and 1 observable, the logical bit being remembered. The last line is the answer: at physical error rate 0.1%, a distance-3 memory of this length flips its logical bit about 5 times per 1000 experiments. Your counts may differ slightly run to run; that is Monte-Carlo noise, not a bug.

**If it fails:**

- `RuntimeError: An attempt has been made to start a new process before the current process has finished its bootstrapping phase` — you dropped the `if __name__ == "__main__":` guard. tqec computes detectors in a process pool, and on macOS child processes re-import your script; without the guard every child re-runs the whole program and the pool explodes recursively. Put the guard back.
- `ValueError: Can't specify separate_observables=True with append_observables=True` — the Stim sampler API takes one or the other. Use `separate_observables=True` alone, as written, to get detectors and observables as two arrays.
- A `Matplotlib is building the font cache` line on first import — harmless, happens once.
- Very different circuit sizes (not 25/48/1) — your tqec version drifted from 0.2.0; the pipeline still works, but re-read the printed numbers instead of trusting this page.

## Stage 3: sweep the error rate with sinter

One data point is an anecdote; the field's calibration artifact is the *curve*: logical error rate versus physical error rate, one curve per code distance. Save as `tqec/hands-on/threshold_plot.py`:

```python
"""Hands-on 1 (part 2): logical error rate vs physical error rate with sinter."""
import matplotlib.pyplot as plt
import sinter
import tqec
from tqec import BlockGraph, compile_block_graph
from tqec.utils.position import Position3D


def make_circuit(k, p):
    graph = BlockGraph("memory")
    c0 = graph.add_cube(Position3D(0, 0, 0), "ZXZ")
    c1 = graph.add_cube(Position3D(0, 0, 1), "ZXZ")
    graph.add_pipe(c0, c1)
    surfaces = graph.find_correlation_surfaces()
    compiled = compile_block_graph(graph, observables=[surfaces[0]])
    return compiled.generate_stim_circuit(
        k=k, noise_model=tqec.NoiseModel.uniform_depolarizing(p)
    )


def main():
    tasks = [
        sinter.Task(
            circuit=make_circuit(k, p),
            decoder="pymatching",
            json_metadata={"k": k, "d": 2 * k + 1, "p": p},
        )
        for k in (1, 2)
        for p in (0.001, 0.002, 0.004)
    ]
    stats = sinter.collect(
        num_workers=4, tasks=tasks, max_shots=100_000, max_errors=200,
        print_progress=False,
    )
    for s in stats:
        print(f"d={s.json_metadata['d']} p={s.json_metadata['p']}: "
              f"{s.errors}/{s.shots} logical errors")
    fig, ax = plt.subplots(1, 1)
    sinter.plot_error_rate(
        ax=ax, stats=stats,
        x_func=lambda s: s.json_metadata["p"],
        group_func=lambda s: f"d={s.json_metadata['d']}",
        failure_units_per_shot_func=lambda s: s.json_metadata["d"],
    )
    ax.set_ylabel("logical error rate per round")
    ax.set_xlabel("physical error rate")
    ax.grid(which="both")
    ax.legend()
    fig.savefig("hands-on/logical_vs_physical.png", dpi=120)
    print("plot saved to hands-on/logical_vs_physical.png")


if __name__ == "__main__":
    main()
```

Run it:

```bash
python hands-on/threshold_plot.py
```

**You should see** (real output, 2026-08-05 — exact counts vary run to run):

```
d=3 p=0.004: 236/3345 logical errors
d=3 p=0.001: 202/39185 logical errors
d=3 p=0.002: 203/10513 logical errors
d=5 p=0.001: 148/100000 logical errors
d=5 p=0.002: 213/17681 logical errors
d=5 p=0.004: 228/3345 logical errors
plot saved to hands-on/logical_vs_physical.png
```

sinter stops each task early once it has 200 errors, which is why shot counts differ per point — it spends your CPU budget where the statistics are cheapest. Open `hands-on/logical_vs_physical.png` and look at the two curves:

- At p = 0.001, distance 5 fails ~0.15% of shots where distance 3 fails ~0.5% — enlarging the code *helped*. That is below-threshold behavior, the phase transition of tier 4, measured by your own laptop.
- The gap between the curves widens as p shrinks. The ratio of logical rates between consecutive distances is the **error-suppression factor Λ** — the single number hardware teams quote to prove their qubits are good enough.

This plot *is* the calibration artifact the community runs on: the Google below-threshold papers are exactly this figure with more distances, more points, and real device noise models instead of uniform depolarizing noise. You now own the full pipeline that produces it.

**If it fails:** the sweep takes a few minutes; distance-5 compilation (k=2) is the slow part, and tqec's README warns that large k "will take a lot of time". If it is too slow, drop `k in (1, 2)` to `k in (1,)` and accept one curve. If sinter reports `decoders that failed` warnings, your PyMatching is older than 2.x — `pip install -U pymatching`.

## Where to go next

- Replace the two memory cubes with a logical CNOT — tqec ships a verified example, `assets/logical_cnot.dae` in the tqec repository, loadable with `BlockGraph.from_dae_file(...)` — and check that two correlation surfaces appear.
- Feed the circuit to Crumble (`print(circuit.to_crumble_url())`) to watch the detectors fire in a browser.
- Next lesson: let a compiler (TopoLS) *design* the blockgraph for you from a circuit, instead of placing cubes by hand.
