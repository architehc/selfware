# Loop recipes by problem type

Dynamic loop configurations, each row measured on real runs (TB3/TB4/vero,
Sep 2026). "Validated" marks rows actually run on both model classes
(qwen38-flash-next 1M and the 27B dense abliterated locals). See
docs/model-playbook.md for endpoint details.

## The matrix

| Problem type | Examples (measured) | Loop recipe | Min model class |
|---|---|---|---|
| **A. Formal proof** | vero, coq-block-bound | write→build→fix EVERY slot; `lake build`/coqc after each write; write-early directive at step 8; no browse phases | flagship (gemini-3.8-flash proven; 27B = 0/173 — do not run) |
| **B. SWE repair** | TB3/TB4 repair tasks | write-early at step 12; verify after every source edit (auto-rescue); VerifierTainted frozen tests; multi-fire budget (2000 iters / 4h) | flagship for hard tasks; 27B acceptable as regression sensor only |
| **C. Visual/CAD** | cad-model, layout-shift | render→vision_analyze→iterate loop; vision profile injected (never model-guessed endpoints); vision calls expected — count them in run summary | VLM flagship (flash-next / gemini / GLM-5.3) |
| **D. Data/ETL pipelines** | production-planning, payments-pipeline | verify=run the pipeline's own checks; explicit "run verification yourself" directive (G4) when no command detectable; long wall budgets | flagship; capability-bound, harness-clean |
| **E. Crypto/forensics** | shadow-relay, memcached-backdoor | observation loops are LEGITIMATE (watch logs, sweep keyspace) — never kill as "verification loop"; huge iteration budgets; no premature VERIFICATION_LOOP_AFTER_EDIT | flagship |
| **F. Review/research (read-only)** | capstone studies, 780k reviews | read-only classification: no mutation mandates, no NoSourceEdit; measured context tiers; final-answer discipline | any incl. 27B (27B is fine here!) |
| **G. Adversarial generation** | red-team corpus | high-temp mass generation, dedup, validate-shape, gate-test; uncensored local only (hosted refuse) | local uncensored 27B (perfect fit) |

## Per-model-class deltas (validated)

| Knob | flagship (flash-next/gemini/GLM) | 27B dense abliterated |
|---|---|---|
| step_timeout_secs | 600–900 | 2400 (measured ~11 t/s + shared prefill) |
| stream_stall_timeout_secs | 600–900 | 1200 (long prefills are legit) |
| max_tokens | 16384 (reasoning room) | 16384 (same, but expect slow decode) |
| write-early threshold | step 8–12 | step 6 (it reads forever otherwise — 0/173 measured) |
| verify-after-write | auto (rescue call) | directive-first (auto only if cheap: lake/cargo) |
| vision | full (proven VLM) | proven but slow — low priority |
| role | solver | sensor / adversarial generator / read-only reviewer |

## Validation status

- A (proof): flagship VALIDATED (gemini 2/9 ×2, replicated); 27B VALIDATED-NEGATIVE (0/173).
- B (repair): flagship PARTIAL (TB4 1/6 GLM, clean capability losses); 27B VALIDATED-NEGATIVE (0/30+).
- C (visual): flash-next RUNNING (cad-model 6 vision calls, iterating).
- D (ETL): flagship VALIDATED (production-planning 15/20 hidden tests).
- E (crypto): flagship VALIDATED-NEGATIVE-but-clean (shadow-relay 4/8, honest long grind).
- F (read-only): all classes VALIDATED (capstone 4/4 natural completions incl. 27B).
- G (adversarial): 27B VALIDATED (500+ cases, 29 gate holes closed).

## Open recipe work (ordered)

1. write-early directive (A, B) — northcode died FAKE_COMPLETE_LOOP never writing.
2. verify-after-every-write for buildable repos (A, B) — gemini's winning cadence, currently model-dependent luck.
3. native-FC auto-fallback on tool-schema 400s (all types, m3:free case).
4. observation-loop exemption for type E (memcached-backdoor GLM was killed watching logs).
