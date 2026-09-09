# Selfware setup and recovery

For endpoint setup, run `selfware boot` to choose a verified recipe card. It supports OpenRouter, Ollama, vLLM/SGLang, and LM Studio. The guided wizard writes configuration after showing the selected recipe; an existing file requires overwrite confirmation. Use `selfware --config <path> boot` to repair a specific configuration. Finish with `selfware llm-doctor` to check the result.

For API keys and credentials, the guided wizard can store an OpenRouter key in the operating system keyring. Keys are scoped to the endpoint origin. `SELFWARE_API_KEY` is an alternative for the selected session. Never paste secrets into setup chat or project documentation. Remote authenticated endpoints require HTTPS by default; the explicit `SELFWARE_ALLOW_PLAINTEXT_REMOTE` opt-in is for a trusted LAN and does not permit embedded credentials in URLs.

For a local Ollama server, start the server and use `ollama list` to find the exact model tag you have pulled. For vLLM, SGLang, or LM Studio, start the server and load a model before running the wizard. Model detection queries the selected local server's `/v1/models`; if it cannot detect a model, the wizard asks for the exact served identifier. Verify connectivity and model availability with `selfware llm-doctor`.

For context and token budgets, set `context_length` to the model's actual configured context window and leave room for `max_tokens`, the response allowance, and safety margin. The server configuration can impose a smaller window than the model's advertised maximum. On vLLM/SGLang check the server's maximum model length; in LM Studio check the context setting. The wizard uses reported server metadata when available and validates generated configuration before writing it.

For configuration errors, `selfware boot --check` reports PASS, FAIL, and SKIP results even when the current TOML cannot load. `selfware boot` provides guided repair independently of the broken configuration. Preserve a copy of any custom settings before authorizing an overwrite. A failed doctor check is not a verified setup: read the failure detail, correct the server or configuration, and verify again.

The optional `selfware boot --chat` assistant uses a small local model and llama-server for setup questions. It provides advice; configuration values come from guided recipe cards. Its documentation is bundled with Selfware and is independent of the current project's docs directory. `selfware boot --check` reports whether the optional model, server, and round-trip checks could actually be performed.
