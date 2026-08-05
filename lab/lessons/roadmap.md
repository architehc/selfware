# How this lab works (0 to hero map)

This lab is a curriculum, not a pile of articles. It takes you from "what is a qubit?" to running real TQEC design tools on your own machine, one dependency at a time. This page explains the machinery: the map, the tiers, the locking, and where your progress lives.

## The map is a graph

Go back to the [curriculum map](#/) (the home page) and look at the shape of it. The map holds **19 lessons**, drawn as boxes on a diagram. Each box is one **node**. The thin lines between boxes are **prerequisites**: a line from lesson A to lesson B means "finish A before B".

You read the map top to bottom. Each horizontal row is a **tier** — a band of lessons at the same conceptual altitude. There are six tiers, numbered 0 through 5. Tier 0 is where you are now; tier 5 is the hands-on finale.

Nothing here is graded. The graph exists for one reason: every lesson assumes exactly the lessons that point into it, and nothing more. If you follow the arrows, you will never meet a term that has not been defined.

## Locked, unlocked, done

Each node on the map is in one of three states, shown by how it is drawn:

- **Locked**: dimmed and faded. Its prerequisites are not finished yet. A locked node cannot be clicked — not as a punishment, but because its lesson would assume things you have not seen.
- **Unlocked**: full color, clickable. All of its prerequisites are done. Click it to open the lesson.
- **Done**: green, with a check mark in the corner. You finished it.

At the bottom of every lesson there is a **mark complete** button. Pressing it flips the node to done, saves your progress, and takes you back to the map — usually unlocking the next row. If you press it by accident, the button becomes *completed ✓*; your progress is yours to manage.

## Where progress is stored

Your progress is stored **in your browser**, in a small built-in key-value store called `localStorage`, under the key `tqec-lab-progress`. Concretely, that means:

- It survives closing the tab, restarting the server, and rebooting your machine.
- It is per-browser and per-device. Open the lab in a different browser and you start fresh.
- There is no account, no server-side state, and nothing leaves your machine.
- To reset everything, clear the site's storage in your browser (for example via the developer tools, or "clear site data" in the browser settings), then reload.

## The tiers, top to bottom

Here is the whole path, so you always know where you are.

- **Tier 0 — Orientation (2 nodes).** *Why TQEC?* (the lesson before this one) and this roadmap. The problem statement and the map legend.
- **Tier 1 — The code (3 nodes).** The surface code as a chessboard, stabilizers and anyons, then boundaries, distance, and logical operators. By the end of this tier you understand what the surface code *is* and why it protects information. All three lessons have an interactive lattice widget.
- **Tier 2 — Decoding (3 nodes).** Syndromes and detectors (how the code talks over time), minimum-weight perfect matching (how the classical computer guesses what broke), and weighted/correlated decoding (how real decoders get smarter). Two of these have a step-through decoder widget.
- **Tier 3 — Computation (3 nodes).** Lattice surgery (how logical qubits interact), space-time plumbing and blockgraphs (how a whole computation becomes a 3D diagram of pipes), and the compilation pipeline (how software turns an algorithm into that diagram).
- **Tier 4 — The full machine (4 nodes).** Magic states and distillation, magic state cultivation, real-time decoding (the latency race between the quantum chip and the classical decoder), and the experiments that crossed the threshold.
- **Tier 5 — Hands-on (4 nodes).** Environment setup, then three practicals where you run the actual open-source tools yourself: **tqec** (blockgraph to logical error rate), **TopoLS** (circuit to pipe diagram), and **Clifft** (simulating magic state cultivation). This is where "0 to hero" cashes out.

Notice the shape of the journey: tiers 0–4 build the mental model, tier 5 spends it. By the time you type your first command, you will know what every part of the output means.

## The widgets

Some lessons end with a small interactive widget embedded in the page. There are two kinds, both built from plain SVG with no dependencies:

- **The lattice playground** draws a surface-code patch you can poke. Buttons labeled **X error**, **Z error**, and **erase** choose what your mouse does; clicking a data qubit applies it. A counter line reports how many errors you placed and how many checks fired. Different lessons load different presets: plain distance-3, distance-3 with anyon labels, distance-5 with a logical-operator overlay, and (in tier 3) a two-patch surgery demo.
- **The decoder stepper** shows a set of fired checks and steps through how a minimum-weight perfect matching decoder pairs them up, one candidate pairing at a time, until it finds the cheapest.

The widgets are not decoration — the lessons are written around them. When a lesson says "Try it", do; the concept will stick far better once your hands have done it.

## The papers

Many lessons end with a *Source papers* footer listing arXiv links. The whole lab is built from a curated 23-paper reading list, and the footers tell you exactly which papers a lesson condenses. They are optional — but they are the real thing, and after tier 2 you will be surprised how much of them you can follow.

## The anatomy of a lesson page

Every lesson page has the same skeleton, so it is worth learning once:

- The **title** matches the box you clicked on the map.
- The **prose** is the lesson itself, with a *Try it* section whenever a widget is present.
- The **widget box** sits at the bottom of the lesson content, above the footer, on lessons that have one.
- The **Key numbers** box is the lesson compressed to its load-bearing facts — the part worth revisiting later.
- The **Source papers** footer lists the arXiv papers the lesson condenses, when there are any.
- The **mark complete** button records your progress, and the **back to map** link returns you to the graph.

The progress summary at the top of the map page (for example "3/19 lessons complete") always tells you how far along the whole graph you are.

## The whole graph on one page

Here is the same map as text, so you can see the dependency shape at a glance (a `──` arrow means "unlocks"):

```
tier 0   why-tqec ── roadmap
tier 1   surface-code ── stabilizers-anyons ── boundaries-distance
tier 2   syndromes-detectors ── mwpm-decoding ── weighted-correlated
tier 3   lattice-surgery ── spacetime-blockgraphs ── compilation-pipeline
tier 4   magic-states ──┬── cultivation ─────────┐
                        └── realtime-decoding ───┴── experiments
tier 5   setup-env ── hands-on-tqec ──┬── hands-on-topols
                                      └── hands-on-clifft
```

Two structural facts to notice. First, tiers 0–3 are a single chain: no branches, no choices, each lesson unlocking exactly one next lesson. Second, the only branches in the whole graph are in tiers 4 and 5 — the magic-state cultivation strand splits off from and rejoins the main line, and the two final hands-on practicals both depend on the tqec practical but not on each other.

## Frequently asked questions

**Can I skip ahead?** Not past locked nodes — the graph enforces prerequisites. If you already know the early material, the intended move is to read quickly and press *mark complete* honestly.

**Does order matter within a tier?** Mostly the tier is a chain anyway (each node's prerequisite is the previous one). Where a tier branches — tier 4 splits into the cultivation and decoding strands before rejoining at *Experiments* — take either branch first; the join node waits for both.

**How long does the whole lab take?** Tiers 0–1 are an evening of reading and playing. Each later tier is similar in reading time but heavier in concept. Tier 5 depends on your machine and how long the tool installs take — budget a separate session for each hands-on.

**What do I need installed?** Nothing for tiers 0–4 beyond a browser. Tier 5's setup lesson walks through the real toolchain (Python, the tqec package, TopoLS, Clifft) step by step.

## How to use this lab well

- Go in order the first time. The graph enforces it anyway, but even re-reading, order is the point.
- Do the widgets. Every "Try it" section names a specific thing to attempt; attempt it.
- Expect tier 2 to be the first real climb. Decoding is where the field's brains live.
- Expect tier 5 to be the most fun. Everything before it is rehearsal.

## If you get stuck

The lab is designed so that confusion has an address. Three moves, in order:

- Re-read the *Key numbers* box of the current lesson — it is the lesson with the prose removed.
- Step back one node on the graph and redo its *Try it* section with the widget. Most "I don't get it" in this subject is really "my hands haven't done it".
- Only then open the source papers in the footer. They are denser than the lessons, but by tier 2 you will find them legible, and reading the original statement of an idea often unsticks what a summary cannot.

## Key numbers

- **19 nodes** in the curriculum graph, arranged in **6 tiers** (0–5).
- Tier sizes: 2 + 3 + 3 + 3 + 4 + 4.
- 2 widget types: the lattice playground (4 presets) and the decoder stepper.
- Progress: one `localStorage` key, `tqec-lab-progress`, stored only in your browser.
- The path ends with you running three real tools: **tqec**, **TopoLS**, and **Clifft**.

## Next

Time to meet the code itself: [the surface code as a chessboard](#/lesson/surface-code).
