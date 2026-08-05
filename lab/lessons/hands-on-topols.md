# Hands-on 2: TopoLS — circuit to pipe diagram

In hands-on 1 you placed two cubes by hand. Real computations have thousands of gates, and nobody draws those blockgraphs manually. **TopoLS** is the front-end compiler that closes the gap: it takes an ordinary quantum circuit and emits a lattice-surgery pipe diagram — cubes, pipes, positions — that tqec can consume directly. This lesson compiles the paper's flagship example, a 16-qubit GHZ state, and hands the result to tqec.

**Verification status: install, the GHZ-16 compilation, and the tqec handoff were all run on macOS (Apple Silicon) on 2026-08-05; outputs shown are real. The d=5-scale benchmarks and the ~33% volume-reduction claim are the authors' numbers from arXiv:2601.23109, not reproduced here.**

Prerequisite: the `tools-env` environment from hands-on 0, activated.

## What TopoLS does, in three stages

From the paper and README, the compiler is a pipeline:

1. **ZX-level topological optimization.** Your circuit becomes a ZX diagram (the spider calculus of tier 2's cousins); **spider fusion** merges connected same-color spiders, collapsing gate-level structure into topological structure. The fused graph is then sliced into layers by connectivity, exposing merge/split operations that a gate list hides.
2. **3D layout via Monte Carlo Tree Search.** Each layer's spiders must become cubes at concrete 3D positions with pipes that do not collide. TopoLS searches the embedding space with MCTS — the same game-tree algorithm family as AlphaGo — optimizing for total space-time volume.
3. **Topology-aware partitioning.** Big circuits are cut along spider-connectivity boundaries so each piece stays small enough for the MCTS stage to handle.

The authors report about **33% average space-time volume reduction** over earlier heuristic compilers, on examples from this 16-qubit GHZ up to 500-qubit random circuits. Treat the number as a claim to test, not a fact to repeat — the reproducibility script (`docs/exp.py`) is right there in the repo.

## Stage 1: clone and install

Keep the clone next to your environment, inside `tqec/`:

```bash
cd tqec
git clone https://github.com/tqec/TopoLS.git
cd TopoLS
../tools-env/bin/pip install -e .
```

**You should see** pip resolve TopoLS's dependencies (PyZX among them) and finish with something like:

```
Successfully installed topols-...
```

**If it fails:** dependency conflicts against the hands-on 0 packages are possible as both projects evolve; if pip cannot resolve, give TopoLS its own venv (`python3 -m venv .venv && source .venv/bin/activate && pip install -e .`) as the upstream README suggests, and run this lesson's commands there. You may also see a harmless `SyntaxWarning: "is" with 'tuple' literal` from `layer_mcts.py` when the package first runs — a known cosmetic wart, not an error.

## Stage 2: compile the 16-qubit GHZ state

The repo ships runnable scripts in `docs/`. The README's quick-start command, **with one documented fix applied** (see the troubleshooting note), is:

```bash
cd docs
../../tools-env/bin/python prog.py -f ghz_16 -b 20 -zx 1 -dir 1 -l 4 -r 0 -s 2 -t 2 -i 1000 -csv result -sp 0 -b0 1
```

The flags name the benchmark (`ghz_16`), the block size (`-b 20`), the ZX optimization (`-zx 1`), MCTS iteration count (`-i 1000`), and so on; `-b0 1` is the workaround flag. Run it and wait — the MCTS search takes about half a minute on a laptop.

**You should see** (real output, 2026-08-05; progress bar elided):

```
Executing ghz_16 benchmark.
Embedding progress:
x_length: 9.0 y_length: 9.0 z_length: 6
Space-time volume: 486.0
Time: 6
Space: 81.0
Compilation time: 25.51264214515686
```

Read the numbers as a floor plan: the compiled computation occupies a 9 × 9 grid of patches for 6 time blocks — 486 units of space-time volume. A CSV summary lands in `result/topols/result.csv`:

```
ghz_16,486,81,6,    ,25.513,20,0,1,1,4,0,2,2,1000
```

**If it fails:** the README documents a **block reference issue** that aborts execution on some inputs; the workaround is exactly the `-b0 1` flag shown above (the README's own quick-start prints `-b0 0`, which is the default — flip it to `1` if you hit the issue; this tutorial already runs with it enabled and it does no harm). If MCTS is too slow, lower `-i 1000` to `-i 200` and accept a slightly worse layout.

## Stage 3: hand the pipe diagram to tqec

TopoLS's `docs/2tqec.py` converts the compilation result into tqec's currency — cube positions, cube kinds (`ZXX`, ...), and pipe endpoints:

```bash
../../tools-env/bin/python 2tqec.py -f ghz_16
```

**You should see** the command finish silently and a new file appear:

```bash
ls result/bgraph/
```

```
ghz_16.bgraph
```

The `.bgraph` file is a Python pickle of two metadata dictionaries (node metadata and edge metadata), not tqec's JSON format — so `BlockGraph.from_json` will *not* read it. The bridge is a short loader you write yourself. Save as `tqec/hands-on/load_topols.py`:

```python
"""Load a TopoLS .bgraph file into a tqec BlockGraph."""
import pickle
from tqec import BlockGraph
from tqec.utils.position import Position3D


def main():
    with open("TopoLS/docs/result/bgraph/ghz_16.bgraph", "rb") as f:
        data = pickle.load(f)
    bgraph_metadata = data["bgraph_metadata"]
    edge_metadata = data["edge_metadata"]

    graph = BlockGraph("ghz_16 from TopoLS")
    for node in bgraph_metadata.values():
        x, y, z = node["position"]
        graph.add_cube(Position3D(int(x), int(y), int(z)), node["tqec"])
    for (p1, p2) in edge_metadata.values():
        graph.add_pipe(Position3D(*map(int, p1)), Position3D(*map(int, p2)))
    print("cubes:", graph.num_cubes, "pipes:", graph.num_pipes)
    surfaces = graph.find_correlation_surfaces()
    print("correlation surfaces:", len(surfaces))


if __name__ == "__main__":
    main()
```

Run it from the `tqec/` directory (the one containing `tools-env/` and `TopoLS/`):

```bash
cd tqec
source tools-env/bin/activate
python hands-on/load_topols.py
```

**You should see** (real output, 2026-08-05):

```
cubes: 202 pipes: 201
correlation surfaces: 11
```

That is the full front end working: a circuit went in one end, and a tqec `BlockGraph` — 202 cubes, 201 pipes, 11 logical correlation surfaces, exactly the 16-qubit GHZ's I/O structure — came out the other. From here the pipeline is hands-on 1 verbatim: pick observables, `compile_block_graph`, `generate_stim_circuit(k=1, ...)`, sample, decode.

**If it fails:**

- `TQECError: JSON file not found` — you called `BlockGraph.from_json` on the `.bgraph`; it is a pickle, use the loader above.
- `ValueError: too many values to unpack` when iterating the edge metadata — iterate `edge_metadata.values()`; each value is the endpoint pair, the keys are TopoLS-internal path names.
- A pickle `ModuleNotFoundError` on load — run the loader with the same venv that produced the file, since pickles can reference the producing package's classes. (The verified file loaded fine with plain `pickle` in `tools-env`.)
- The blockgraph is a research artifact; if tqec's validator complains about exotic cube kinds on other benchmarks, that is the known rough edge of a 2026 academic release, not your mistake.

## What to take away

The pipeline from the front-end survey is now real and on your disk: **circuit → ZX graph → pipe diagram → tqec BlockGraph → Stim circuit → decoded logical error rate**. TopoLS owns the middle segment, and its volume numbers decide how many factories and patches a computation costs — which is why a claimed 33% volume reduction matters enough to verify yourself. Next lesson: what happens when the circuit contains gates Stim cannot even represent.
