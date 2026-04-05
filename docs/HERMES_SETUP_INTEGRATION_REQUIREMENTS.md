# Hermes Setup Integration Requirements

## Purpose

Define requirements for a more robust Hermes setup flow when integrating against custom OpenAI-compatible endpoints, browser tooling, optional voice tooling, and local command execution environments.

This is based on an observed real setup session that:

- installed Playwright system dependencies with `sudo`
- installed browser binaries and extra Ubuntu packages mid-wizard
- configured a custom endpoint successfully
- failed optional NeuTTS installation because the Hermes venv had no `pip`
- continued after partial failures without a strong final integration-health summary

## Observed Failure Modes

1. A Python virtual environment existed, but a later installer step used a different venv without `pip`.
2. Optional tool setup required privileged package installation during the interactive wizard.
3. Large downloads and package installs happened opportunistically instead of being planned up front.
4. Some capabilities were partially configured, but the setup finished with weak guarantees about what was actually usable.
5. The wizard mixed core requirements and optional add-ons in one long session.
6. The runtime banner showed counts of tools and skills, but not which ones were merely available vs fully configured and healthy.

## Requirements

### P0: Core Reliability

1. The setup must preflight all required runtimes before entering the interactive wizard.
   Acceptance:
   - verify `python`, `venv`, `ensurepip` or `pip`, `node`, `npm`, `git`, and `curl`
   - fail early with actionable remediation if any mandatory runtime is missing

2. The setup must guarantee that every managed Python virtual environment contains a working `pip`.
   Acceptance:
   - after creating a venv, run a validation step for `python -m pip --version`
   - if missing, automatically run `python -m ensurepip --upgrade` or equivalent bootstrap
   - optional package installation must not proceed until `pip` is confirmed working

3. The setup must separate mandatory setup from optional integrations.
   Acceptance:
   - core install completes even if TTS, image generation, browser cloud, or RL integrations are skipped
   - optional tool failures do not leave core install in an ambiguous state
   - final output clearly distinguishes `core_ready` vs `optional_failed`

4. The setup must provide a deterministic post-install validation report.
   Acceptance:
   - run health checks for configured provider, command backend, browser tooling, and enabled optional tools
   - print pass/fail per subsystem
   - exit non-zero when required subsystems are not usable

5. The setup must support non-interactive execution for CI and scripted integrations.
   Acceptance:
   - all wizard choices can be supplied via flags, env vars, or a config file
   - no prompt may block unattended installation when non-interactive mode is enabled
   - non-interactive mode must emit machine-readable results

### P0: Privilege and Dependency Control

6. The setup must declare privileged operations before starting them.
   Acceptance:
   - show a consolidated list of `sudo` operations up front
   - allow the user to approve, skip, or defer them as a batch
   - do not surprise the user with mid-flow privilege escalation

7. The setup must support a `--no-sudo` or deferred-system-deps mode.
   Acceptance:
   - browser and speech features can be marked `pending system deps`
   - the install can finish without root
   - the tool prints an exact follow-up command list for later completion

8. The setup must distinguish system packages from user-space packages and browser binaries.
   Acceptance:
   - system packages, Python packages, Node packages, and browser downloads are reported separately
   - failures are scoped to the failing layer, not collapsed into a generic setup error

### P1: Endpoint and Provider Robustness

9. The setup must validate custom OpenAI-compatible endpoints beyond `/models`.
   Acceptance:
   - verify connectivity, model listing, and a minimal chat/completions probe when supported
   - report detected model name and effective context length if available
   - persist provider metadata in a stable format

10. The setup must make current provider state unambiguous.
    Acceptance:
    - do not display a stale “current model/provider” while also saying no provider is configured
    - clearly show `detected`, `configured`, and `default after save`

11. The setup must tolerate missing API keys for optional services without noisy partial configuration.
    Acceptance:
    - skipped providers remain explicitly `disabled`
    - no half-written provider section is left behind

### P1: Browser and Automation Readiness

12. Browser automation setup must support a staged installation model.
    Acceptance:
    - phase 1: Node package presence
    - phase 2: browser binary download
    - phase 3: system library installation if required
    - each phase can be retried independently

13. The setup must verify browser automation with a real smoke test.
    Acceptance:
    - after Playwright install, run a short headless launch test
    - report success against the local runtime actually configured for Hermes

14. The setup must support headless Linux and WSL explicitly.
    Acceptance:
    - detect WSL/headless environments
    - recommend or auto-enable `xvfb` only when needed
    - explain why extra packages are being installed

### P1: Optional Tool Installation Safety

15. Optional tool installers must run inside the intended Hermes runtime, not an arbitrary shell environment.
    Acceptance:
    - all package installs use the Hermes-managed interpreter path
    - the interpreter path is printed before install
    - the tool verifies the package import after installation

16. Optional tool failures must produce exact remediation commands.
    Acceptance:
    - errors include the failing interpreter path
    - commands are copy-pastable and environment-specific
    - the final summary marks the feature as disabled until validation succeeds

### P1: Resume and Idempotency

17. The setup must be resumable.
    Acceptance:
    - interrupted installs can restart without redoing completed work unnecessarily
    - completed steps are cached with explicit validation markers

18. The setup must be idempotent.
    Acceptance:
    - rerunning setup should not duplicate symlinks, config entries, or skill copies
    - unchanged components should be reported as `already valid`

### P2: UX and Integration Quality

19. The wizard should offer a concise “integration profile” path.
    Example profiles:
    - local dev
    - CI/headless
    - custom OpenAI-compatible endpoint
    - browser-enabled agent

20. The setup should provide a dry-run mode.
    Acceptance:
    - prints packages to install, files to create, env vars to set, and commands requiring root
    - performs no writes

21. The setup should emit a machine-readable install manifest.
    Acceptance:
    - JSON or YAML report listing configured providers, enabled tools, versions, pending items, and validation results

22. The setup should end with a single readiness summary.
    Acceptance:
    - `ready`, `ready_with_optional_failures`, or `not_ready`
    - include exact blocking items for `not_ready`

23. The runtime UI should distinguish inventory from readiness.
    Acceptance:
    - show separate counts for `registered`, `configured`, and `healthy` toolsets
    - show enabled provider/model and whether validation passed in this environment
    - do not imply full readiness from a large tool/skill count alone

## Recommended Minimum Acceptance Suite

The integration setup should not ship without these tests:

1. Fresh Linux install with no Python venvs present.
2. Existing Hermes install rerun idempotently.
3. Custom OpenAI-compatible endpoint configured non-interactively.
4. `--no-sudo` path where browser deps are deferred.
5. Optional TTS install where `pip` is initially missing in the managed venv.
6. WSL/headless setup with browser smoke validation.
7. Full setup interrupted mid-run, then resumed successfully.

## Immediate Improvements Suggested by the Transcript

1. Add a preflight `python -m pip --version` check for every Hermes-managed venv before any optional package install.
2. Move all `sudo apt install` work into an explicit dependency phase before the wizard, or allow deferral.
3. Add a final readiness table with `core`, `browser`, `tts`, `search`, `provider`, and `skills hub` rows.
4. Add a non-interactive installer path for custom endpoint integration.
5. Add a browser smoke test and a TTS import smoke test.
6. Add a startup health panel that reports `core ready`, `provider verified`, `browser verified`, `tts verified`, and `optional integrations pending`.
