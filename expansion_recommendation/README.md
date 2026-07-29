# expansion_recommendation

Predefined, **random-access, preloaded** loop-modeling examples — one file per selfware
component — for a **touch-screen visual-coding** environment where the user navigates a
spatial canvas of loop-stage nodes with gestures.

- **29 components · 580 examples** (20 per component).
- Grounded in each component's real source (authored while read in the context of the
  full engine — the ~600k budget framing).
- `index.json` is the random-access manifest (component → tier, loop stage, example ids,
  stages/gestures covered). `_schema.json` defines the file and example shape.

## Tiers
- **full** (ships in the engine bulk): agent, tools, evolve, cognitive, safety, config,
  analysis, orchestration, evolution, api, session.
- **tooling** (in the graph, reachable via expand, not shipped in bulk): ui, computer,
  testing, observability, devops, doctor, llm_doctor, bin, resource, input, output,
  supervision, self_healing, mcp, lsp, consolidation, templates, interview.

## What each example teaches
For a given component, an example says: which **loop stage** it serves, the **pattern**,
how it **reshapes the loop** (control / state / budget), which **loop-objects** it touches,
how you **wire** it (inputs/outputs), the **touch interaction** (gesture → canvas action →
visual), a **mini scenario**, and a **pitfall** (an invariant to preserve).

Load `index.json`, pick a component, random-access its `example_ids`, and hydrate examples
on demand as the user drags that component onto the loop canvas.
