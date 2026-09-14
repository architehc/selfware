# Reading notes: sections 3.4–3.7, printed pp. 22–35

Source: https://arxiv.org/pdf/2609.11873. Read downloaded text and visually inspected Figure 5. Claims about referenced studies below are the survey authors’ reports, not independently verified results. Page 36 starts applications, assigned elsewhere.

## Main point

The useful distinction is between improving an agent and improving the procedure that makes later agents. The paper explicitly separates a loop’s structure from evidence that the loop works. Its strongest conclusion is bounded meta-improvement; sustained, statistically reliable compounding across generations under comparable resources remains open (pp. 32, 35).

## What the authors mean

- **L3: decide what to learn from next (pp. 22–26).** Current weaknesses/history must influence subsequent experience acquisition, persistent learning must occur, and the changed learner must affect later acquisition. New synthetic samples or merely visiting new states after a policy update do not establish this. Selecting from an existing pool can qualify. Human-written selection rules can qualify. Examples include executable tasks in AZR, theorem conjectures near a prover’s frontier, and Voyager’s practice curriculum plus reusable skills.
- **L4: decide what operational experience changes persist (pp. 26–31).** Deployment evidence becomes reusable memory, skills, tools, harness changes, or weights, and later tasks actually inherit these changes. The authors organize this into trajectory distillation, system revision, and admission/maintenance/release of updates. Human release authority may remain. A useful saved artifact is insufficient if the executor never retrieves or follows it (pp. 29–31).
- **L5: persistently revise the process responsible for future improvement (pp. 31–35).** Editable objects include the improver, successor evaluator, research policy, or successor-generation procedure. Merely editing task-solving code does not suffice. A curriculum crosses this boundary when its persistent selection procedure itself is revised and reused (pp. 33–34).

## The precise test for meta-improvement

The paper distinguishes **structural L5** from **effective L5** (p. 32).

1. Identify the exact revised mechanism and motivating evidence.
2. Show it persists and is invoked to generate/evaluate/select a later successor. That establishes the structural claim.
3. Compare old and revised mechanisms starting from comparable agents and evidence, with matched resources including evaluation costs.
4. Independently assess resulting successors; freeze the revised mechanism during transfer tests to isolate what it learned about improving.
5. Track multi-round gains, retention, transfer, cost, harmful updates, and stopping decisions (pp. 34–35, Table 8).

My assessment: this is a useful experimental checklist, not a mathematical theorem or universal quantitative threshold. The manuscript distinguishes a mechanism existing from that mechanism reliably helping, but does not settle how much evidence establishes broad RSI.

## Evidence and its limits, as reported

- **STOP (p. 32):** a Python improver optimizes its own source; a selected fourth-generation version beats its seed on five transfer tasks. Weaker-model runs regress on average, and some candidates evade soft budgets or exploit evaluation bugs.
- **Gödel Agent (p. 33):** both task and update code can change. Fourteen of 100 MGSM trials finish below the starting policy; unrestricted runs can call stronger models, complicating attribution.
- **DGM (p. 33):** task-agent descendants evolve, but archive management and parent selection remain fixed. The paper calls it a transition case, explicitly refusing to infer a better improvement mechanism from task gains alone.
- **HyperAgents (p. 33):** revised meta-agents transfer to unseen mathematics grading; the 200-iteration experiment does not establish a statistically significant final advantage for transferred initialization.
- **RQGM (p. 33):** successor evaluators change between epochs against an independent ground-truth anchor; incompatible scores are discarded. Reported held-out pass rates are 71.7% versus 69.9%, with lower search-token use. Its stability argument is restricted to frozen epochs.
- **A-Evolve-Training (p. 34):** four rounds on a 30B model revise a persistent research policy after development scores decouple from external gains. Final reported score 0.86 versus 0.87 for the best human entry supports bounded policy revision, not autonomous objective invention.
- **AIDE2 (p. 34):** company reports seven accepted harness improvements in 100 unattended steps and external transfer; its stronger experiment finds no statistically significant efficiency advantage when the evolved agent runs the outer search.

## Where the boundaries are less clean — my assessment

L3 (curriculum control), L4 (deployment persistence), and L5 (mechanism revision) are meaningful dimensions, but their portrayal as a single ordered ladder needs care. A deployed assistant may retain useful lessons without choosing its practice curriculum; an offline optimizer may revise its own search code without live deployment. The text itself qualifies classification as mechanism-specific (pp. 22, 25, 32). I would ask about each dimension separately before assigning a system one highest level.

Artifact names cannot classify systems: a “skill” can solve tasks or author future skills; a rubric can judge task outputs or select successors. Even external objectives persist at L5. Therefore “fixed human objective” alone cannot explain an L2/L5 boundary. The decisive question is the role of the changed component in later improvement.

The evidence section is more careful than the title: it openly records negative and statistically inconclusive results. It does not demonstrate runaway acceleration, removal of human governance, or reliable unbounded compounding.

## Figures worth reading together

- Figure 5, p. 23: two L3 feedback loops, task generation/self-play and autonomous practice. I visually checked the code example; it is consistent when diagram alignment is preserved.
- Figure 6, p. 24: qualitative map of automation in data pipelines; explicitly not measured scores.
- Figure 7, p. 27: pagination example shows experience becoming a tool, a harness revision, or an admitted skill. Useful concrete bridge to agent engineering.
- Figure 8, p. 32: L4 changes agent behavior; L5 changes and inherits the improvement process. Its upward path illustrates possible progress, not measured acceleration.
- Table 8, p. 35: strongest compact checklist for testing recursion. No central numbered mathematical equation in these sections; the main formalism is verbal loops and evaluation dimensions.

## Three discussion questions

1. What would convince us that a successor is better at *improving*, rather than merely better at solving its present tasks?
2. Are L3/L4/L5 a true ladder, or should curriculum control, deployment learning, and meta-improvement be separately scored dimensions?
3. If an evaluator evolves, which independent reference stays protected so that higher scores still mean better successors?
