# Reading together: The Last AI Built by Humans

Source: [arXiv:2609.11873v1](https://arxiv.org/pdf/2609.11873), dated September 10, 2026. Reading notes begun September 12, 2026. Page references below use the printed page numbers, which match PDF page numbers.

These are discussion notes about the paper, not an independent replication of its experiments or verification of every cited source. Authors' claims and our assessments are distinguished. The immediate scope is understanding the paper before deciding whether to apply anything to selfware.

## Reading map

| Pages | Topic | Reading responsibility |
|---|---|---|
| 1–2 | Abstract, overview, contents | Shared orientation |
| 3–12 | Motivation, HCI, definitions, evidence scope | Foundations agent |
| 12–22 | B0, L1, L2 | Foundations agent |
| 22–35 | L3, L4, L5, evaluation | Recursion agent |
| 36–44 | Science, robotics, software, healthcare | Applications/evidence agent |
| 44–50 | Industry cases | Applications/evidence agent |
| 50–53 | Challenges and conclusion | Main agent |
| 54–66 | References | Lookup index; underlying sources not exhaustively checked |
| 67–75 | Landscape and system directory | Applications/evidence agent |

The complete source was downloaded and extracted into 75 page files at `/tmp/selfware-paper-2609.11873/`. That temporary cache may disappear; the source link and these notes provide the durable reading record. Text extraction emitted PDF font/syntax warnings, so layout-sensitive findings require checking the actual page image.

Detailed agent notes: [foundations and HCI](foundations.md), [L3–L5 and recursion](recursion.md), and [applications and industry evidence](applications-evidence.md).

One early caution: Figure 3's post-2026 curves are explicitly illustrative. Equation 4 mechanically closes 78% of each domain's remaining benchmark headroom; these are not measured RSI gains or fitted forecasts (pp. 8–9).

## Starting point for our discussion

The paper's central unit is the **improvement loop**, rather than an individual model or algorithm. Ask what changes, what the successor inherits, and who controls the decisions that produce the next change (pp. 9–12).

An illustrative coding example, constructed for this discussion:

1. An agent fixes a bug in today's task. That alone does not demonstrate persistent self-improvement.
2. The agent retains a validated debugging skill and uses it on later tasks. This introduces persistent adaptation.
3. It revises the procedure that diagnoses failures and creates debugging skills, then uses that revised procedure in later improvement rounds. This can meet the paper's structural L5 criterion.
4. The revised procedure produces better subsequent improvements under comparable total budgets and independent evaluation. This is the additional evidence needed for effective L5.

The precise L1–L4 classification depends on which decisions the AI controls, not simply on whether an artifact is saved. The paper states the structural/effective L5 distinction explicitly on p. 32.

## The autonomy ladder

| Level | Responsibility transferred to the AI |
|---|---|
| B0 | Refine the current output; no persistent system change |
| L1 | Execute prescribed improvements that persist |
| L2 | Choose improvement strategies/interventions |
| L3 | Decide what future learning experience to acquire based on the learner |
| L4 | Turn deployment experience into persistent adaptation |
| L5 | Revise and reuse the mechanism responsible for later improvements |

This paraphrases the boundary summary on p. 35. A higher level is a classification of autonomy; it does not establish greater capability, efficiency, reliability, or safety. The authors explicitly separate these properties.

## Main-agent notes: challenges, pp. 50–53

The authors identify eight connected research directions:

1. **Diagnose the component that actually needs repair.** A failure can originate in data, context, tools, the model, or the evaluator. Controlled interventions and component freezing should establish causality rather than merely find a compensating patch (p. 51).
2. **Measure the learning value of experience.** Validity, difficulty for the current learner, and durable learning benefit are different properties. Compare adaptive acquisition against learner-independent schedules under matched acquisition and training budgets (p. 51).
3. **Manage inherited state.** Retaining a skill does not show that it is retrieved, followed, or helpful. Growing libraries can impair retrieval; evaluation must distinguish update quality, activation, faithful execution, and downstream benefit (p. 51).
4. **Adapt validation to the domain.** Software tests depend on specification and coverage; scientific results depend on protocols and instruments; physical and clinical interventions can have consequences that a software rollback cannot reverse (pp. 51–52).
5. **Keep evaluator evolution credible.** An editable internal evaluator needs assessment against independent acceptance criteria. Otherwise changing scores can reflect changing measurement or exploitation. Separate generation changes from evaluation changes in controlled comparisons (p. 52).
6. **Test inherited improvement capacity over time.** Evaluate the mechanism through the successors it produces, on fresh tasks and across repeated runs. Include the cost of developing and evaluating the mechanism, rejected updates, regressions, and recovery (p. 52).
7. **Count the full cost and human contribution.** More search, a stronger base model, maintenance, review, or rework may explain apparent gains. Report time and cost to validated capability, and investigate when to stop unproductive search (pp. 52–53).
8. **Preserve reproducible inheritance artifacts.** Record parent state, proposed change, evidence, evaluation configuration, acceptance, and later use. Executable evidence and formal proofs still only support claims relative to the experiment or specification that was actually checked (p. 53).

The authors explicitly note that the AIDE2 study did not establish a statistically significant efficiency benefit when an evolved harness became the outer improver (p. 52). That qualification matters when interpreting the title.

**Our assessment:** this is most useful as a framework for asking what a self-improvement claim actually demonstrates. Its own conclusion calls for longitudinal evidence of transferable gains under explicit budgets and oversight (p. 53). Reading it as proof of open-ended accelerating RSI would exceed the evidence the authors say they have.

## Questions to carry through the reading

- What persistent state changes after one round, and can we trace its actual use in the next?
- Does the intervention improve task execution, or the process that finds future interventions?
- How much observed gain comes from additional compute, stronger models, human assistance, or evaluator access?
- What evidence survives fresh tasks, protected evaluation, multiple runs, and full cost accounting?
- Are the autonomy levels sufficiently distinct, or should some properties be recorded separately?

Begin the shared close reading with section 1.2 (pp. 3–4), then the formal loop anatomy and definition in section 2.2 (pp. 9–11). This establishes the vocabulary before the long system survey.
