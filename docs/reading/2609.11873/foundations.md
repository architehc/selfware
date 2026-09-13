# Foundations reading notes — printed pp. 3–21

Source: https://arxiv.org/pdf/2609.11873, The Last AI Built by Humans: Toward Genuine Recursive Self-Improvement. Read sections 1–3.3; section 3.4 begins printed p. 22. This is a reading and critique of the paper, not independent replication of its citations. No repository changes or implementation recommendations.

## What the authors are trying to establish

The introduction (pp. 3–7) argues that development bottlenecks extend beyond training compute: collecting informative experience, diagnosing failures, evaluating changes, and coordinating repeated updates still require expensive human work. RSI is proposed as a way to internalize more of that coordination. The useful unit of analysis is therefore an entire improvement loop, not a particular model, algorithm, or self-editing code demonstration.

Their definition (pp. 10–11) requires acquired experience to produce persistent changes to the AI system and for those changes to affect subsequent improvement. The system boundary includes weights, code, harnesses, memories, tools, policies, and improvement machinery. A candidate passing the current acceptance rule and being inherited is called an improvement (p. 10); this is an operational acceptance definition, not a guarantee of real-world benefit.

The strongest conceptual distinction is structural versus effective recursion (p. 7): showing that a revised improvement mechanism is retained and reused differs from showing that it produces better successors under comparable budgets and independent evaluation. This distinction should anchor the entire reading.

## B0 / L1 / L2 in ordinary language

- B0 (pp. 12–14): make this answer or artifact better within the present task; no persistent system change benefits future independent tasks. Reflection and retries alone are insufficient.
- L1 (pp. 14–18): follow a human-designed improvement recipe, validate its products, and retain the products for later use. Humans specify both the recipe and acceptance criteria.
- L2 (pp. 18–21): use feedback to choose the next intervention, test it, and retain successful changes. Humans still fix the task boundary, objective, and acceptance rule; the surrounding search engine may also remain fixed.

The memorable distinction is output revision, prescribed persistent updating, then feedback-driven choice of updating strategy. L2 ranges from prompt search (GEPA) to executable harness search (ADAS/AFlow), training-program experiments (autoresearch), and kernel optimization. Editing impressive-looking objects is not itself evidence that the improver improved.

## HCI: what is measured and what is assumed

Section 2.1 (pp. 7–9) compiles 393 model–benchmark observations across ten domains. The authors group compatible benchmark/evaluation protocols and use weighted source consensus. Source-type weights are 3, 2.5, 2, and 1, with an additional 0.75 factor for first-party reports. Only 17 of 33 observations in their latest-model audit entered the trajectories; incompatible observations were retained separately.

For a consensus benchmark score s and the benchmark's entry-year 90th-percentile frontier F, HCI = 100*(s-F)/(100-F). A benchmark-year frontier is again its 90th-percentile HCI, and each domain averages these frontiers with square-root model-count weights. HCI=50 means half of the benchmark-score gap from that benchmark's entry frontier to perfect score has closed. It does not mean 50% of human ability, 50% of AGI, or 50% autonomy.

Authors' reported 2026 HCI values include graduate science 85.8, software engineering 52.6, and tool agents 39.9 (p. 9). Their motivation is that persistent, interactive workflows retain substantial room for improvement.

The post-2026 region of Figure 3 is explicitly illustrative. Equation 4 sets each domain's endpoint to 100 - 0.22*(100-T_2026), mechanically closing 78% of every domain's remaining headroom. These curves are neither measured RSI gains nor an estimated causal forecast. The claim that weak domains improve more is built into that construction.

## Critical assessment and assumptions worth discussing

1. HCI is a normalization convention, not a cross-domain difficulty model. Different baselines and ceilings can produce different HCI values for identical raw-score gains. Perfect benchmark score may also be an inappropriate stand-in for complete domain competence. Source reliability weights and square-root coverage weights are pragmatic choices; they do not by themselves establish robustness.
2. Benchmark composition, coverage, and protocols matter. A domain can change as eligible benchmark families and sampled models change. The authors disclose protocol filtering, but the reported cybersecurity line still includes changed subsets/pass@1 aggregation. A visually smooth trajectory should not erase that caveat.
3. The graph motivates RSI but does not isolate it. Observed benchmark gaps do not establish that RSI closes those gaps, nor how much it helps relative to additional ordinary training, tool engineering, or inference compute.
4. There is a useful tension between the strict definition and the broad roadmap. Strict RSI involves improving future improvement; L1 and many L2 examples deliberately do not revise their governing improvement procedure. Treat these as constituent mechanisms or stages toward stronger RSI, not demonstrations that every surveyed system satisfies the full definition.
5. System boundaries deserve scrutiny. Pages 10–11 caution that improving an external artifact is not necessarily self-improvement. Yet some L1 examples (pp. 16–17), such as infrastructure repairs, model evaluation, and application integration, are described mainly as persistent downstream artifacts. For each example we should ask whether the artifact actually returns to the improver's state or merely benefits its customer's system.
6. The taxonomy concerns who controls improvement decisions, but Figure 3 also loosely associates task domains with autonomy levels (p. 9). Strong benchmark performance alone cannot demonstrate control over future improvement decisions. Keep task capability separate from demonstrated improvement-loop autonomy.
7. The evidence is intentionally heterogeneous (p. 12): papers, technical reports, repositories, documentation, engineering blogs, and company research materials. The authors acknowledge on p. 21 that company-reported results are not equivalent to independent replication. The introduction's concrete performance figures are reported case studies, not results independently reproduced in this survey.
8. Their evaluation cautions are substantial, not boilerplate: persistence can preserve regressions; adaptive evaluation access can turn a benchmark into training feedback; seed cherry-picking and evaluator leakage can inflate progress. The central question is independent future performance per comparable total budget, including evaluation and human work.

## Three discussion questions

1. What must survive into the next independent task for us to call it self-improvement: a remembered tip, a tested harness change, a weight update, or a better method for finding the next change?
2. If the agent rewrites its prompts while a fixed human evaluator selects winners, what new evidence would make us believe the improvement process itself became better?
3. How much of Figure 3's argument survives if we remove its illustrative future curves and compare RSI against ordinary improvement with equal compute and human-review budgets?

## Suggested first reading stop

Read the definition on p. 11 alongside the three loop questions on p. 10, then contrast B0/L1/L2. This provides vocabulary before the long catalog of systems. When returning to the introduction's claims, label each as motivation, structural mechanism evidence, performance evidence, or speculation.
