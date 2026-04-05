# Hermes Setup Implementation Checklist

Related requirements: [HERMES_SETUP_INTEGRATION_REQUIREMENTS.md](/home/ivo/selfware/docs/HERMES_SETUP_INTEGRATION_REQUIREMENTS.md)

## Goal

Turn the setup and first-run experience into a deterministic integration flow with:

- explicit runtime preflight
- predictable privilege handling
- non-interactive support
- subsystem health verification
- accurate runtime readiness reporting

## Priority Order

### Phase 0: Ship Blockers

1. Add venv and `pip` preflight for every Hermes-managed Python environment.
2. Add a final readiness report with pass/fail per subsystem.
3. Separate core install from optional integrations.
4. Add `--no-sudo` and deferred system dependency mode.
5. Add non-interactive setup for custom OpenAI-compatible endpoint configuration.

### Phase 1: Integration Hardening

6. Add staged browser setup with real smoke validation.
7. Add optional integration validation for TTS and other add-ons.
8. Make provider state reporting unambiguous.
9. Add resumable and idempotent setup step tracking.

### Phase 2: UX and Operations

10. Add dry-run mode.
11. Emit machine-readable install manifests.
12. Improve runtime banner to distinguish inventory from readiness.
13. Add integration profiles like `local-dev`, `ci-headless`, `custom-openai`, `browser-enabled`.

## Issue Breakdown

### Issue 1

Title: `setup: validate and bootstrap pip in every managed venv`

Why:
- the observed setup failed NeuTTS install because Hermes used a venv without `pip`

Scope:
- after venv creation, validate `python -m pip --version`
- if missing, run `python -m ensurepip --upgrade`
- fail clearly if `pip` still cannot be bootstrapped

Acceptance:
- optional Python package installs never start without a working `pip`
- error output includes interpreter path and remediation command

Dependencies:
- none

Priority:
- P0

### Issue 2

Title: `setup: split core install from optional integrations`

Why:
- core readiness is currently mixed with browser, TTS, image, and RL setup

Scope:
- define `core` vs `optional` setup phases
- core phase covers config dir, runtime, provider, CLI command, and base health checks
- optional integrations run only after core is marked ready

Acceptance:
- core install can finish successfully even when optional integrations are skipped or fail
- final summary marks optional failures without downgrading core readiness

Dependencies:
- Issue 1

Priority:
- P0

### Issue 3

Title: `setup: add deterministic post-install readiness report`

Why:
- the setup currently completes with partial failures but no strong final health contract

Scope:
- add a final table with rows like `core`, `provider`, `browser`, `tts`, `search`, `skills hub`
- each row must be `ready`, `disabled`, `pending`, or `failed`
- exit non-zero if required subsystems are not ready

Acceptance:
- setup ends in one of `ready`, `ready_with_optional_failures`, `not_ready`
- blocking items are explicit

Dependencies:
- Issue 2

Priority:
- P0

### Issue 4

Title: `setup: introduce explicit privileged dependency phase`

Why:
- Playwright and speech dependencies triggered root package installs in the middle of the wizard

Scope:
- gather all required `sudo` operations before execution
- support approve-all, skip-all, or defer-all
- print exact deferred commands for later

Acceptance:
- no surprise mid-flow privilege escalation
- `--no-sudo` leaves affected features in `pending system deps`

Dependencies:
- none

Priority:
- P0

### Issue 5

Title: `setup: support non-interactive configuration for custom OpenAI-compatible endpoints`

Why:
- integration and CI workflows need repeatable setup without prompts

Scope:
- allow endpoint URL, model, context length, and optional API key through flags/env/config
- add a non-interactive validation flow
- persist provider metadata deterministically

Acceptance:
- custom endpoint setup can run unattended end-to-end
- result includes machine-readable validation output

Dependencies:
- Issue 3

Priority:
- P0

### Issue 6

Title: `setup: stage browser installation and add smoke validation`

Why:
- browser support currently mixes Node packages, system libs, and browser downloads in one opaque step

Scope:
- split browser setup into package, system dependency, binary download, and smoke test stages
- run a short headless launch test against the actual Hermes runtime

Acceptance:
- failures identify the exact stage
- successful setup proves browser launch works, not just that packages were downloaded

Dependencies:
- Issue 4

Priority:
- P1

### Issue 7

Title: `setup: validate optional integrations inside the managed runtime`

Why:
- optional tool installs should verify importability and runtime correctness, not just package install success

Scope:
- run `import` smoke tests for optional Python integrations
- verify configured interpreter path before install
- store validation markers per optional integration

Acceptance:
- TTS and similar integrations are marked `ready` only after validation succeeds

Dependencies:
- Issue 1

Priority:
- P1

### Issue 8

Title: `setup: make provider state reporting unambiguous`

Why:
- the setup transcript showed confusing state between detected provider, current provider, and configured provider

Scope:
- define `detected`, `configured`, `active`, and `validated` provider states
- use those labels consistently in setup and runtime UI

Acceptance:
- no stale or contradictory provider messaging
- runtime banner can show provider validation status explicitly

Dependencies:
- Issue 5

Priority:
- P1

### Issue 9

Title: `setup: add resumable and idempotent step tracking`

Why:
- large downloads and multi-phase installs should survive interruption cleanly

Scope:
- persist step completion markers with validation hashes or status flags
- skip already-valid steps on rerun
- re-run only invalid or incomplete phases

Acceptance:
- interrupted setup can resume without duplicating work
- rerunning setup reports `already valid` where appropriate

Dependencies:
- Issues 2, 4, 6

Priority:
- P1

### Issue 10

Title: `setup: add dry-run and install manifest output`

Why:
- integration teams need to inspect effects before running setup on CI, WSL, or managed hosts

Scope:
- dry-run mode prints packages, downloads, config writes, env vars, and root actions
- emit JSON or YAML manifest of configured and validated subsystems

Acceptance:
- dry-run performs no writes
- manifest can be consumed by automation

Dependencies:
- Issue 3

Priority:
- P2

### Issue 11

Title: `ui: show registered vs configured vs healthy tools at startup`

Why:
- the runtime banner currently shows inventory counts, not readiness

Scope:
- add separate counters for `registered`, `configured`, and `healthy`
- show provider/model validation state in the same panel

Acceptance:
- startup screen does not imply full readiness from inventory count alone

Dependencies:
- Issues 3, 8

Priority:
- P2

### Issue 12

Title: `setup: add integration profiles for common deployment modes`

Why:
- most users fit a small number of setup shapes and should not answer every question manually

Scope:
- add profiles such as `local-dev`, `ci-headless`, `custom-openai`, `browser-enabled`
- each profile preselects reasonable defaults while remaining editable

Acceptance:
- a user can complete a common integration path with materially fewer prompts

Dependencies:
- Issue 5

Priority:
- P2

## Recommended Delivery Sequence

Week 1:
- Issue 1
- Issue 2
- Issue 3

Week 2:
- Issue 4
- Issue 5
- Issue 6

Week 3:
- Issue 7
- Issue 8
- Issue 9

Week 4:
- Issue 10
- Issue 11
- Issue 12

## Definition of Done

The setup integration work should be considered done when all of these are true:

1. Fresh install succeeds without hidden privilege escalation.
2. Managed venvs always have a validated `pip`.
3. Core readiness is reported independently from optional integrations.
4. Custom endpoint setup can run non-interactively.
5. Browser and optional integrations have real smoke validation.
6. Setup can resume safely after interruption.
7. Runtime UI distinguishes inventory from health.
