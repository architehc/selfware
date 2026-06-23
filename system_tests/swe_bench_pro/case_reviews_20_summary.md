# 20-Case Agent Review Summary — SWE-bench Pro

## Method
- Spawned 20 coder subagents, one per instance from `sample_50.jsonl` (first 20 instances).
- Each agent read the instance metadata, checked existing predictions (Kimi 256k, GLM 5.2 v2, GLM full-sample), and returned a case review.
- Reviews are read-only; no containers or API calls were run.

## Instances reviewed
1. `gravitational__teleport-1b08e7d0dbe68fe530a0f08ad408ec198b7c53fc-vee9b09fb20c43af7e520f57e9239bbcf46b7113d`
2. `gravitational__teleport-3ff75e29fb2153a2637fe7f83e49dc04b1c99c9f`
3. `gravitational__teleport-1a77b7945a022ab86858029d30ac7ad0d5239d00-vee9b09fb20c43af7e520f57e9239bbcf46b7113d`
4. `gravitational__teleport-24cafecd8721891092210afc55f6413ab46ca211-vee9b09fb20c43af7e520f57e9239bbcf46b7113d`
5. `gravitational__teleport-5dca072bb4301f4579a15364fcf37cc0c39f7f6c`
6. `navidrome__navidrome-56303cde23a4122d2447cbb266f942601a78d7e4`
7. `navidrome__navidrome-dfa453cc4ab772928686838dc73d0130740f054e`
8. `navidrome__navidrome-29b7b740ce469201af0a0510f3024adc93ef4c8e`
9. `navidrome__navidrome-b3980532237e57ab15b2b93c49d5cd5b2d050013`
10. `future-architect__vuls-e52fa8d6ed1d23e36f2a86e5d3efe9aa057a1b0d`
11. `future-architect__vuls-407407d306e9431d6aa0ab566baa6e44e5ba2904`
12. `future-architect__vuls-be7b9114cc9545e68fb0ee7bc63d7ec53d1a00ad`
13. `future-architect__vuls-2c84be80b65d022c262956cd26fc79d8bb2f7010`
14. `flipt-io__flipt-518ec324b66a07fdd95464a5e9ca5fe7681ad8f9`
15. `flipt-io__flipt-a0cbc0cb65ae601270bdbe3f5313e2dfd49c80e4`
16. `flipt-io__flipt-21a935ad7886cc50c46852be21b37f363a926af0`
17. `flipt-io__flipt-e594593dae52badf80ffd27878d2275c7f0b20e9`
18. `flipt-io__flipt-b2170346dc37cf42fda1386cd630f24821ad2ac5`
19. `flipt-io__flipt-c12967bc73fdf02054cf3ef8498c05e25f0a18c0`
20. `NodeBB__NodeBB-97c8569a798075c50e93e585ac741ab55cb7c28b-vf2cf3cbd463b7ad942381f1c6d077626485a1e9e`

---

## Highest-Impact Findings

### 1. Evaluation harness applies the patch in the wrong order
**Instance:** `teleport-5dca072b`
- The `evaluate_predictions.py` entryscript currently runs:
  ```
  git apply patch.diff
  <before_repo_set_cmd>   # includes git reset --hard + checkout of test files
  run tests
  ```
- The `before_repo_set_cmd` resets the repo to base and checks out the test files from the fix commit, **wiping out the predicted patch** before tests run.
- This means many prior 0/5 evaluations may be false negatives.
- **Fix:** move `git apply -v /workspace/patch.diff` to **after** `before_repo_set_cmd`.

### 2. Prompt truncation / partial file context is the dominant failure mode
Repeated across Teleport, Navidrome, Vuls, and Flipt cases:
- Agentless prompts claim to include a file as “FULL FILE” but budget truncation omits the target function, causing hallucinated signatures (e.g., unary `DeleteMFADevice` instead of streaming).
- GLM v2 literally saw only the first ~57 lines of `lib/sshutils/x11/display.go` and rewrote the struct with wrong field names.
- **Fix:** extract and inject the exact function/region to edit instead of relying on whole-file inclusion; scale budgets with `context_window` and verify the target region is present.

### 3. Test-patch context is almost never shown to the model
Agents consistently recommended:
- Apply the `test_patch` to the working tree **before** generation, or include the new failing assertions verbatim in the prompt.
- Include expected inputs/outputs from `fail_to_pass` tests as a structured “test oracle.”
- This matters for boundary checks (`>=` vs `>`), exact error strings, epoch/version formatting, and new file creation.

### 4. Cross-file / package-level changes are mishandled
Examples:
- `navidrome-29b7b740`: `utils/cache/simple_cache.go` + `cached_http_client.go` must change together.
- `flipt-b2170346`: `checker.go` + `grpc.go` + `auth.go` signature change.
- `vuls-be7b9114`: `models/library.go` + `contrib/trivy/pkg/converter.go` + `scanner/library.go`.
- **Fix:** when a failing test targets a Go package, retrieve **all `.go` files in that package** and run a compile check before tests.

### 5. New files are not created
Examples:
- `flipt-a0cbc0c`: `internal/config/testdata/envsubst.yml` must be created.
- `flipt-e594593`: `internal/cue/extended.cue` must be created.
- `navidrome-b398053`: `core/agents/lastfm_test.go` must be created.
- **Fix:** instruct the model explicitly that it may create new files, and include new-file diffs (`--- /dev/null`) in the patch parser.

### 6. Missing test-feedback / repair loop
Most models generate a single patch and stop. Cases with subtle semantics need:
- Run `fail_to_pass` tests on the patch.
- Feed stderr/diff back to the model with a requirements checklist.
- Ask the model to reconcile every bullet in `requirements`.

---

## Model-Specific Observations

| Model | Observation |
|-------|-------------|
| **Kimi K2.7 Code 256k** | Produces larger, more applyable patches than GLM; still misses exact test contracts (boundary `>=`, error-message format, full-path parsing). Gets close on `teleport-1a77b794` but fails exact semantics. |
| **GLM 5.2** | Suffers heavily from prompt truncation; hallucinates struct rewrites; empty or mis-targeted patches common. |
| **GLM 5.2 full-sample** | Only ran a few Teleport instances before exiting; many empty/short patches due to workspace path allow-list issues and max-iteration loops. |

---

## Recommended Next Steps

1. **Fix `evaluate_predictions.py` patch ordering** — highest ROI; may reclassify prior results.
2. **Inject `test_patch` / failing assertions into the generation prompt** instead of only applying it at evaluation time.
3. **Retrieve complete packages** for cross-file Go changes and run `go build ./...` before tests.
4. **Add a test-feedback loop**: apply patch → run `fail_to_pass` → feed failures back → repair.
5. **Support new-file creation** in the patch extractor and prompt.
6. **Verify target function presence** in the prompt before calling the model.
7. **Re-run Kimi 256k and GLM 5.2** after harness fixes to measure true pass rates.

---

## Raw Reviews

### teleport-1b08e7d0 (X11 display full socket path)
- **Root cause:** Incomplete implementation of `ParseDisplay` for full Unix-domain paths. Kimi fixed `unixSocket()` but not parsing; GLM hard-coded `/tmp/.X11-unix` for darwin.
- **Recommendation:** Include the `unix_socket_full_path` subtest contract in the prompt; ensure `display.go` is not truncated.

### teleport-3ff75e29 (Delete last MFA device)
- **Root cause:** Target function omitted from prompt; model hallucinated unary `DeleteMFADevice` signature. Selfware run hit path allow-list issues.
- **Recommendation:** Inject exact `DeleteMFADevice` streaming method region; add repo path to agent allow-list.

### teleport-1a77b794 (MongoDB 48MB message size)
- **Root cause:** Models miss exact boundary (`>=` vs `>`) and error-string format. GLM only changed the constant to 48MB.
- **Recommendation:** Add requirements checklist and test-feedback loop.

### teleport-24cafecd (SQL Server TDS bounds check)
- **Root cause:** Low-level parser bug; fuzz seed corpus tests not surfaced.
- **Recommendation:** Include `fuzz_test.go` and run `go test -run '^FuzzMSSQLLogin$|FuzzMSSQLLogin/seed#'`.

### teleport-5dca072b (Kube proxy ClientCAs too large)
- **Root cause:** **Harness wipes patch before evaluation.** Kimi also edited wrong file (`lib/auth/middleware.go`).
- **Recommendation:** Fix evaluation ordering; instruct model to use existing `t.ClusterName` field.

### navidrome-56303cde (R128 gain tags)
- **Root cause:** Wrong file ranked; agents edit `conf/configuration.go` instead of `scanner/metadata/metadata.go`. Missing exact conversion formula.
- **Recommendation:** Surface `getR128GainValue` test table; restrict edits to metadata scanner.

### navidrome-dfa453cc (playlist membership operators)
- **Root cause:** Multi-file change across `model/criteria/json.go` and `operators.go`; exact SQL subquery shape hidden.
- **Recommendation:** Include `operators_test.go` assertions in prompt.

### navidrome-29b7b740 (SimpleCache options)
- **Root cause:** Cross-file API refactor spanning `simple_cache.go` and `cached_http_client.go`.
- **Recommendation:** Retrieve entire `utils/cache/` package; compile check.

### navidrome-b3980532 (Last.FM default API key)
- **Root cause:** Problem statement underspecified; requires new `Enabled` flag, new test file, and hard-coded fallback API key not in requirements.
- **Recommendation:** Include `test_patch` in prompt; run `go test ./core/agents/...`.

### vuls-e52fa8d6 (Vuls2 schema version check)
- **Root cause:** Ordering trap in `shouldDownload`; early `SkipUpdate` return before schema check.
- **Recommendation:** Include `detector/vuls2/db_test.go` and list `fail_to_pass` subtests.

### vuls-407407d3 (Trivy-to-Vuls severity consolidation)
- **Root cause:** Wrong file ranked (parser vs converter); consolidation rules live in test fixture.
- **Recommendation:** Feed agent expanded test fixture and exact expected output.

### vuls-be7b9114 (PURL propagation)
- **Root cause:** Cross-file change across `models/library.go`, `contrib/trivy/pkg/converter.go`, `scanner/library.go`.
- **Recommendation:** Include updated test fixtures in retrieval context.

### vuls-2c84be80 (RPM source package parsing)
- **Root cause:** Signature change in `splitFileName`; error should become warning and continue; epoch formatting nuance.
- **Recommendation:** Include test oracle with expected `models.Package`/`SrcPackage` outputs.

### flipt-518ec324 (CORS string-to-slice whitespace)
- **Root cause:** Bug in mapstructure decode hook; model may search CORS files instead of `internal/config/config.go`.
- **Recommendation:** Apply `test_patch` before generation to show expected assertion.

### flipt-a0cbc0c (envsubst config hook)
- **Root cause:** Requires creating new fixture `internal/config/testdata/envsubst.yml`.
- **Recommendation:** Explicitly tell model it can create files; include new-file diff support.

### flipt-21a935ad (gRPC log level config)
- **Root cause:** Cross-file: `config/config.go` + `cmd/flipt/main.go` wiring; integration test `TestServeHTTP`.
- **Recommendation:** Add cross-file checklist for new config fields; run `fail_to_pass` after patch.

### flipt-e594593 (CUE validation line numbers)
- **Root cause:** Requires new `internal/cue/extended.cue` + `validate.go` logic + testdata update.
- **Recommendation:** Surface failing `TestValidate_Extended` output and required line numbers.

### flipt-b2170346 (audit token resource)
- **Root cause:** Cross-file signature change between `grpc.go` and `auth.go`; compile not checked.
- **Recommendation:** Run `go build ./internal/cmd/...` before selected tests; require requirement-to-files checklist.

### flipt-c12967bc (gRPC context.Canceled status)
- **Root cause:** Need `errors.Is` before `status.FromError` short-circuit; auth interceptor also affected.
- **Recommendation:** Include new `TestErrorUnaryInterceptor` table entries in prompt; require interceptor enumeration.

### NodeBB-97c8569a (user API privacy leak)
- **Root cause:** Cross-file: controller + new `hidePrivateData` helper; test patch not visible during generation.
- **Recommendation:** Apply `test_patch` before generation.

