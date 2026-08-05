# Hands-on 0: environment setup

The theory tiers are done; now you need the machines. This lesson installs the four tools the rest of the hands-on track uses — **tqec** (blockgraph compiler), **Stim** (stabilizer simulator), **PyMatching** (MWPM decoder), and **sinter** (Monte-Carlo sampler) — into one isolated Python environment. Everything happens in a terminal, and everything lives inside a *virtual environment* so nothing touches your system Python.

**Verification status: every command on this page was run on macOS (Apple Silicon) on 2026-08-05. The outputs shown are the real observed outputs.**

## Step 1: check your Python

tqec requires Python 3.10 through 3.13 — it does *not* yet support 3.14, and 3.9 is too old. Check what you have:

```bash
python3 --version
```

**You should see** something in the supported range:

```
Python 3.12.13
```

**If it fails:** any version outside 3.10–3.13 will bite you at install time with `ERROR: No matching distribution found for tqec`. On macOS the system `python3` is often 3.9, which is too old — the fix is a newer interpreter from Homebrew (`brew install python@3.12`), after which use `python3.12` instead of `python3` in the commands below. On Linux, use your distribution's `python3.12` package. The rest of this lesson writes `python3`; substitute your interpreter name everywhere.

## Step 2: create the virtual environment

From the repository root, create the environment inside `tqec/tools-env/` (this directory is git-ignored — it is a runtime artifact, never committed):

```bash
cd tqec
python3 -m venv tools-env
source tools-env/bin/activate
```

**You should see** your shell prompt gain a `(tools-env)` prefix after the `activate` line. That prefix is the guardrail: every `pip` and `python` command from now on stays inside the sandbox. To leave the environment later, type `deactivate`.

**If it fails:** `command not found: python3` means no interpreter is on your PATH — install one first (step 1). If activation fails with a permissions error, check you are in the `tqec/` directory and that `tools-env/bin/activate` exists (`ls tools-env/bin`).

## Step 3: install the tool stack

One command installs everything — tqec, Stim, PyMatching, sinter, and matplotlib for plots:

```bash
pip install tqec stim pymatching sinter matplotlib
```

**You should see** a long download progress ending in a line like:

```
Successfully installed contourpy-... tqec-0.2.0 stim-1.16.0 pymatching-2.4.0 sinter-1.15.0 matplotlib-3.11.1 ...
```

The exact micro-versions will drift; the five names above are what matter.

**If it fails:** `ERROR: No matching distribution found for tqec` is the Python-version problem from step 1 — tqec's package metadata declares `Requires-Python >=3.10,<3.14`, so pip refuses it on an unsupported interpreter. Recreate the venv with a supported Python. A `Matplotlib is building the font cache` message on first use is harmless; it happens once.

## Step 4: verify the install

```bash
python -c "import tqec, stim, pymatching; print('tool stack ready')"
```

**You should see:**

```
tool stack ready
```

For the record, print the versions — these are the ones this tutorial was verified against:

```bash
python -c "
import importlib.metadata as m
for p in ['tqec', 'stim', 'pymatching', 'sinter', 'matplotlib']:
    print(p, m.version(p))
"
```

**You should see** (verified 2026-08-05):

```
tqec 0.2.0
stim 1.16.0
pymatching 2.4.0
sinter 1.15.0
matplotlib 3.11.1
```

**If it fails:** `ModuleNotFoundError` means the install went to a different Python than the one you are running — confirm the `(tools-env)` prefix is in your prompt, and re-run step 3. If `import tqec` works but your versions are far newer than the ones above, expect small API differences; tqec explicitly warns it is under active development with no backwards-compatibility guarantee, so prefer the printed API in the next lesson over anything you remember from older docs.

## What you just installed, in one sentence each

- **tqec** — compiles a lattice-surgery blockgraph (cubes and pipes) into a fault-tolerant Stim circuit.
- **Stim** — simulates stabilizer circuits with Pauli noise, at kilohertz shot rates.
- **PyMatching** — decodes detector syndromes by minimum-weight perfect matching (the blossom algorithm of tier 4, industrialized).
- **sinter** — stitches Stim to PyMatching and sweeps error rates across your CPU cores.
- **matplotlib** — draws the logical-vs-physical error-rate plot that is the whole point of the exercise.

Next lesson: build an actual topological computation and measure its logical error rate.
