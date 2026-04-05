# Hermes Setup GitHub Issue Drafts

Related docs:
- [HERMES_SETUP_INTEGRATION_REQUIREMENTS.md](/home/ivo/selfware/docs/HERMES_SETUP_INTEGRATION_REQUIREMENTS.md)
- [HERMES_SETUP_IMPLEMENTATION_CHECKLIST.md](/home/ivo/selfware/docs/HERMES_SETUP_IMPLEMENTATION_CHECKLIST.md)

## Milestones

### Milestone 1: Core Setup Reliability

Issues:
- 1. Validate and bootstrap `pip` in every managed venv
- 2. Split core install from optional integrations
- 3. Add deterministic post-install readiness report
- 4. Introduce explicit privileged dependency phase
- 5. Support non-interactive custom endpoint setup

Goal:
- make setup deterministic and integration-safe on fresh machines, CI, WSL, and custom endpoint deployments

### Milestone 2: Integration Hardening

Issues:
- 6. Stage browser installation and add smoke validation
- 7. Validate optional integrations inside the managed runtime
- 8. Make provider state reporting unambiguous
- 9. Add resumable and idempotent step tracking

Goal:
- make optional capabilities verifiable instead of best-effort

### Milestone 3: UX and Operational Readiness

Issues:
- 10. Add dry-run and install manifest output
- 11. Show registered vs configured vs healthy tools at startup
- 12. Add setup profiles for common deployment modes

Goal:
- improve operator visibility and reduce setup friction

---

## Issue 1

### Title
`setup: validate and bootstrap pip in every managed venv`

### Labels
`setup` `python` `reliability` `p0`

### Milestone
`Core Setup Reliability`

### Body
#### Summary
Hermes setup currently assumes every managed Python virtual environment has a working `pip`. In the observed setup flow, an optional NeuTTS install failed because the Hermes-managed venv existed but `python -m pip` was unavailable.

#### Problem
- optional Python integrations can fail after setup appears healthy
- the failure happens late and is harder to diagnose than a preflight error

#### Scope
- after creating or selecting a managed venv, run `python -m pip --version`
- if missing, attempt `python -m ensurepip --upgrade`
- if still missing, fail with explicit remediation
- print the interpreter path in all errors

#### Acceptance Criteria
- optional Python package installation never starts without verified `pip`
- failures include exact interpreter path and next-step command
- setup exits non-zero if a required venv cannot be bootstrapped

#### Dependencies
- none

---

## Issue 2

### Title
`setup: split core install from optional integrations`

### Labels
`setup` `ux` `reliability` `p0`

### Milestone
`Core Setup Reliability`

### Body
#### Summary
The current setup flow mixes core readiness with optional integrations like TTS, browser automation, image generation, and RL tooling. This makes partial failure hard to reason about.

#### Problem
- users can finish setup without knowing whether the core agent is actually ready
- optional failures pollute the main install path

#### Scope
- define `core` and `optional` setup phases
- core includes config dir, runtime checks, CLI command, terminal backend, and main provider
- optional integrations run after core is marked ready
- final status must distinguish core readiness from optional failures

#### Acceptance Criteria
- core setup can succeed while optional integrations are skipped or fail
- final report clearly shows `core_ready` and optional statuses separately
- setup logic does not treat optional integration failure as silent success

#### Dependencies
- issue 1

---

## Issue 3

### Title
`setup: add deterministic post-install readiness report`

### Labels
`setup` `health` `ux` `p0`

### Milestone
`Core Setup Reliability`

### Body
#### Summary
Setup should finish with a deterministic subsystem health report instead of a loosely summarized success message.

#### Problem
- current output does not establish a strong contract for what is actually usable

#### Scope
- add a final table with subsystem rows such as `core`, `provider`, `browser`, `tts`, `search`, `skills hub`
- status values should be `ready`, `disabled`, `pending`, or `failed`
- define final setup result as `ready`, `ready_with_optional_failures`, or `not_ready`

#### Acceptance Criteria
- setup ends with a single readiness summary
- required subsystem failure causes non-zero exit
- optional failures remain visible and actionable

#### Dependencies
- issue 2

---

## Issue 4

### Title
`setup: introduce explicit privileged dependency phase`

### Labels
`setup` `system-deps` `security` `p0`

### Milestone
`Core Setup Reliability`

### Body
#### Summary
Browser and speech setup currently trigger `sudo apt install` mid-wizard. Those privileged operations should be declared and controlled explicitly.

#### Problem
- privilege escalation happens unexpectedly
- users cannot plan or defer system-level changes cleanly

#### Scope
- collect required `sudo` operations before execution
- support approve-all, skip-all, and defer-all modes
- add `--no-sudo` support
- emit exact deferred commands

#### Acceptance Criteria
- no surprise privilege prompts during setup
- deferred system dependencies are recorded as `pending system deps`
- setup can complete core flow without root

#### Dependencies
- none

---

## Issue 5

### Title
`setup: support non-interactive custom OpenAI-compatible endpoint configuration`

### Labels
`setup` `integration` `provider` `p0`

### Milestone
`Core Setup Reliability`

### Body
#### Summary
Integration and CI use cases need a scripted way to configure a custom OpenAI-compatible endpoint and verify it.

#### Problem
- current setup path is interactive-heavy
- custom endpoint integration is hard to automate reliably

#### Scope
- accept endpoint URL, model, context length, and optional API key through flags, env vars, or config
- validate `/models` and run a minimal provider probe where possible
- persist provider metadata deterministically

#### Acceptance Criteria
- custom endpoint setup can run unattended
- validation results are available in machine-readable form
- configured provider state is stable across reruns

#### Dependencies
- issue 3

---

## Issue 6

### Title
`setup: stage browser installation and add smoke validation`

### Labels
`setup` `browser` `playwright` `p1`

### Milestone
`Integration Hardening`

### Body
#### Summary
Browser setup should be decomposed into explicit stages and end with a real launch test.

#### Problem
- package install, browser download, and system dependencies are currently mixed together
- success can mean “downloaded” rather than “usable”

#### Scope
- split browser setup into:
  - node package phase
  - browser binary phase
  - system dependency phase
  - smoke test phase
- add a short headless launch test

#### Acceptance Criteria
- failures identify the exact stage
- successful setup proves browser startup works in the configured environment

#### Dependencies
- issue 4

---

## Issue 7

### Title
`setup: validate optional integrations inside the managed runtime`

### Labels
`setup` `python` `optional-tools` `p1`

### Milestone
`Integration Hardening`

### Body
#### Summary
Optional integrations should only be marked ready after validation inside the actual Hermes runtime.

#### Problem
- package installation success is not enough to prove runtime correctness

#### Scope
- verify importability for optional Python integrations after install
- use the Hermes-managed interpreter path for all validation
- persist validation markers per integration

#### Acceptance Criteria
- TTS and similar integrations are marked ready only after import or smoke validation passes
- setup prints the interpreter path used for install and validation

#### Dependencies
- issue 1

---

## Issue 8

### Title
`setup: make provider state reporting unambiguous`

### Labels
`setup` `provider` `ux` `p1`

### Milestone
`Integration Hardening`

### Body
#### Summary
The setup and runtime UI should distinguish detected, configured, active, and validated provider state.

#### Problem
- current output can imply contradictory provider state

#### Scope
- define provider lifecycle labels
- update setup messaging and runtime views to use them consistently
- surface provider validation state in the runtime UI

#### Acceptance Criteria
- setup output no longer shows stale or contradictory provider information
- runtime banner can display provider validation explicitly

#### Dependencies
- issue 5

---

## Issue 9

### Title
`setup: add resumable and idempotent step tracking`

### Labels
`setup` `reliability` `state-management` `p1`

### Milestone
`Integration Hardening`

### Body
#### Summary
Long-running setup steps should survive interruption and rerun cleanly.

#### Problem
- package downloads and multi-stage setup are expensive to repeat
- current setup has weak restart semantics

#### Scope
- persist validated step markers
- skip already-valid steps on rerun
- rerun only incomplete or invalid phases

#### Acceptance Criteria
- interrupted installs can resume cleanly
- rerunning setup does not duplicate side effects
- valid steps are reported as already complete

#### Dependencies
- issues 2, 4, 6

---

## Issue 10

### Title
`setup: add dry-run and install manifest output`

### Labels
`setup` `ops` `ci` `p2`

### Milestone
`UX and Operational Readiness`

### Body
#### Summary
Operators need a way to preview and audit setup actions before running them on CI, WSL, or managed hosts.

#### Problem
- current setup is write-oriented and not easy to inspect beforehand

#### Scope
- add dry-run mode
- emit JSON or YAML manifest with versions, enabled tools, pending items, and validation results

#### Acceptance Criteria
- dry-run performs no writes
- manifest is machine-readable and complete enough for automation

#### Dependencies
- issue 3

---

## Issue 11

### Title
`ui: show registered vs configured vs healthy tools at startup`

### Labels
`ui` `health` `setup` `p2`

### Milestone
`UX and Operational Readiness`

### Body
#### Summary
The runtime banner should distinguish tool inventory from actual readiness.

#### Problem
- a high tool or skill count can imply more readiness than the environment really has

#### Scope
- show separate counts for `registered`, `configured`, and `healthy`
- surface provider/model validation status
- make pending or failed integrations visible at startup

#### Acceptance Criteria
- startup UI does not imply readiness from inventory count alone
- health state is visible without opening config files

#### Dependencies
- issues 3, 8

---

## Issue 12

### Title
`setup: add profiles for common deployment modes`

### Labels
`setup` `ux` `profiles` `p2`

### Milestone
`UX and Operational Readiness`

### Body
#### Summary
Most users fit a few common setup shapes and should not have to answer every prompt manually.

#### Problem
- the current wizard is thorough but high-friction for common integration paths

#### Scope
- add profiles such as:
  - `local-dev`
  - `ci-headless`
  - `custom-openai`
  - `browser-enabled`
- prefill reasonable defaults while allowing override

#### Acceptance Criteria
- common setup paths require materially fewer prompts
- profile behavior remains explicit and reviewable

#### Dependencies
- issue 5

