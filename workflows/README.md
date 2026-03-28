Product workflow set inspired by `garrytan/gstack` and `alirezarezvani/claude-skills`.

Files:
- `product_discovery.yml`: discovery brief plus assumption risk register
- `product_delivery.yml`: architecture, sprint plan, and review gate
- `product_release.yml`: release checklist, docs plan, and launch note
- `product_build.yml`: orchestrates the three workflows above

Examples:

```bash
selfware workflow validate workflows/product_build.yml

selfware workflow run workflows/product_build.yml \
  --input idea="Daily briefing app for founders" \
  --input target_user="Busy founders" \
  --input constraints="Rust backend, web frontend, ship in one week" \
  --input repo_context="Selfware repo with workflow and agent runtime" \
  --input verification_command="cargo test --quiet" \
  --input smoke_test_command="cargo test --quiet" \
  --dry-run
```

For the remote 122B endpoint, the checked-in `selfware-auto-txn545-Qwen3-5-122B-A10B-NVFP4.toml`
is the safest starting point because it already disables backend thinking.
