#!/usr/bin/env python3
"""Generate per-model OpenRouter configs from openrouter_models.toml.

Reads system_tests/projecte2e/config/openrouter_models.toml and emits one
system_tests/projecte2e/config/openrouter_<model>.toml per profile.

The generated configs respect:
  - context_length (sets agent.token_budget and agent.context_window for prompt
    truncation / context compression)
  - max_output_tokens (sets max_tokens sent to the API)
  - temperature
  - tier (small/medium/large) for default iteration/time budgets
  - minimal flag (enables cheap-model tuning)

The API key is intentionally a placeholder; set SELFWARE_API_KEY at runtime.
"""
import re
import sys
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parent
REGISTRY = ROOT / "config" / "openrouter_models.toml"
OUT_DIR = ROOT / "config"


def sanitize_filename(name: str) -> str:
    """Produce a safe filename from a profile key."""
    return re.sub(r"[^a-zA-Z0-9_.-]+", "_", name).strip("_")


def get_int(profile: dict, key: str, fallback_key: str | None = None) -> int:
    """Read an integer field, falling back to a legacy key if present."""
    if key in profile:
        return int(profile[key])
    if fallback_key and fallback_key in profile:
        return int(profile[fallback_key])
    raise KeyError(f"missing required field: {key}")


def tier_defaults(tier: str) -> tuple[int, int]:
    """Return (max_iterations, step_timeout_secs) defaults for a tier."""
    tier = str(tier).lower()
    if tier == "large":
        return 40, 900
    if tier == "medium":
        return 35, 750
    # small / default
    return 30, 600


def generate_config(profile_key: str, profile: dict) -> str:
    model_id = profile["id"]
    max_tokens = get_int(profile, "max_output_tokens", "max_tokens")
    context_length = get_int(profile, "context_length", "context_window")
    temperature = float(profile["temperature"])

    # Use per-profile iteration/timeout if set, otherwise derive from tier.
    tier = profile.get("tier", "small")
    default_iterations, default_timeout = tier_defaults(tier)
    max_iterations = int(profile.get("max_iterations", default_iterations))
    step_timeout_secs = int(profile.get("step_timeout_secs", default_timeout))

    # Clamp token budget to a safe default; keep existing 32k cap for backwards
    # compatibility unless the model is explicitly large-tier.
    budget_cap = 32768 if tier != "large" else min(context_length, 131072)
    token_budget = min(context_length, budget_cap)

    tool_style = profile.get("tool_style", "auto")
    # XML-style tool models (Qwen, some others) produce cleaner output when
    # streaming and native function calling are both disabled.
    use_xml_tools = tool_style == "xml"

    # For small / minimal models, disable the LLM-based context compressor so
    # fragile models do not hit JSON-parse failures during compression.
    is_small = tier == "small" or bool(profile.get("minimal"))

    lines = [
        '# Auto-generated from config/openrouter_models.toml',
        '# Do NOT edit by hand; rerun generate_openrouter_configs.py',
        '# Do NOT put the real API key here; set SELFWARE_API_KEY in the environment.',
        '',
        f'endpoint = "https://openrouter.ai/api/v1"',
        f'model = "{model_id}"',
        f'max_tokens = {max_tokens}',
        f'temperature = {temperature}',
        f'api_key = "$SELFWARE_API_KEY"',
        '',
        '[agent]',
        f'max_iterations = {max_iterations}',
        f'step_timeout_secs = {step_timeout_secs}',
        f'token_budget = {token_budget}',
        'context_window = 0' if is_small else f'context_window = {context_length}',
        'native_function_calling = false',
        f'streaming = {"false" if use_xml_tools else "true"}',
        'min_completion_steps = 2',
        'require_verification_before_completion = true',
    ]

    if profile.get("minimal"):
        lines.extend([
            '',
            '# Minimal/cheap-model tuning: strip memory, limit tools, enforce early edit deadline.',
            'disable_episodic_memory = true',
            'minimal_tool_catalog = true',
            'edit_deadline_step = 6',
            'max_no_edit_steps = 6',
        ])

    lines.extend([
        '',
        '[retry]',
        'max_retries = 8',
        'base_delay_ms = 2000',
        'max_delay_ms = 60000',
        '',
        '[safety]',
        'allowed_paths = ["./**", "/app/**"]',
        '',
        '[metadata]',
        f'tier = "{tier}"',
        f'tool_style = "{profile.get("tool_style", "auto")}"',
        f'recommended = {"true" if profile.get("recommended") else "false"}',
    ])

    notes = profile.get("notes")
    if notes:
        # Escape any double quotes in notes so the TOML string stays valid.
        safe_notes = str(notes).replace('"', '\\"')
        lines.append(f'notes = "{safe_notes}"')

    lines.append('')
    return "\n".join(lines)


def main() -> int:
    if not REGISTRY.exists():
        print(f"ERROR: registry not found: {REGISTRY}", file=sys.stderr)
        return 1

    with REGISTRY.open("rb") as f:
        registry = tomllib.load(f)

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    generated = 0

    for section in ("coding_models", "vision_models"):
        for key, profile in registry.get(section, {}).items():
            filename = sanitize_filename(f"openrouter_{key}.toml")
            out_path = OUT_DIR / filename
            out_path.write_text(generate_config(key, profile))
            print(f"Generated {out_path}")
            generated += 1

    print(f"Generated {generated} OpenRouter config files in {OUT_DIR}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
