# Read-only artifact harness

`selfware artifact-harness` is an opt-in, artifact-scoped analysis path for
repository tasks that benefit from several narrow model contexts. It is a
separate runtime from `selfware run` and `selfware multi-chat`:

The design adapts the artifact-owned contexts, dynamic activation, and directed
coordination described in [HERMES / Dev-Primitives](https://arxiv.org/html/2610.07832v1),
with an explicit capability and data-egress boundary around the first phase.

```bash
selfware artifact-harness "trace authentication failures across the request path"
selfware artifact-harness "review cache invalidation" --rounds 3 --max-artifacts 6
selfware artifact-harness "review the parser" --output-format json
selfware artifact-harness "review the parser" --allow-remote-source-egress
```

The first implementation is intentionally read-only. It can produce grounded
evidence-matched model observations and coordinate bounded metadata between
selected artifacts. It cannot edit files, execute commands, run tests, call
tools, activate an arbitrary path, or decide that a coding task succeeded.

## Architecture

1. A bounded local census discovers eligible source files independently
   of repository metadata. A saved evolve graph may rank or connect only paths
   already present in that eligible set; it cannot grant file-read authority.
2. Selfware pins the canonical requested root for each artifact read and opens
   every path component descriptor-relatively with `O_NOFOLLOW`. Candidate
   source paths containing symlinks, devices, denied paths, multiply linked
   files, oversized files, private coding-agent state, or components outside
   the requested root are rejected.
3. The activation plan is frozen and bound to the Git HEAD tree, the full
   SHA-256 of the graph bytes that were parsed, and the full SHA-256 of each
   selected artifact. Dirty working-tree content is covered by the artifact
   hashes rather than inferred from Git.
4. Each model call receives one measured, sanitized, numbered artifact slice.
   Rust files also receive a bounded signature skeleton. Calls use the
   auxiliary chat API with no tool definitions.
5. A reported finding is labeled `evidence_matched` only when its quote matches
   a delivered line. Its title and severity remain untrusted model assessments.
6. With `--rounds 2` or `3`, the invocation authorizes typed coordination among
   the bounded active set. A worker can propose only a recipient, message kind,
   and exact source evidence. The broker verifies that evidence, derives safe
   identifiers, and forwards fixed metadata (sender, kind, line/hash, and
   identifiers). It drops evidence excerpts and accepts no free-form message
   body. Graph edges never authorize a route.
7. Selfware rechecks the Git tree, graph bytes, and every selected artifact
   after each round. A mismatch discards that round's claims and reports
   `binding_changed`.

The default is one round and four artifacts. `--rounds` accepts 1 through 3;
later rounds run only recipients of accepted typed messages. `--max-artifacts`
accepts 1 through 8. A final-round worker cannot emit another message.

## Authority boundary

The model-facing worker type contains messages and a bounded side-call client.
It does not contain an `Agent`, `ToolRegistry`, filesystem handle, shell,
execution sandbox, hook runner, MCP client, or mutation adapter. All file reads
happen in the deterministic harness. Source reads used to assemble model
context happen before a prompt is built, against a run-local read attestation
for an exact snapshot; later deterministic reads only recheck that binding.

Repository text, graph labels, and model summaries remain tainted. Source text
passes through credential redaction and the prompt-injection trust gate before
model use. Inter-worker messages contain no model prose, quotes, or source
excerpts. They may contain a small bounded set of ASCII identifiers derived by
the broker from verified evidence. Framing prevents source text from closing
its data block.

The configured model transport is a separate egress boundary. Loopback model
endpoints are allowed by default. For any non-local endpoint, the command fails
before repository discovery unless `--allow-remote-source-egress` is present.
That flag authorizes sending selected source slices, relative paths, and content
hashes to the configured provider for this run; the report records the choice.
Artifact calls ignore environment proxies and never follow redirects, so a
local or approved endpoint cannot silently replay a source-bearing POST to a
second authority. Remote source egress also requires HTTPS unless the existing
`SELFWARE_ALLOW_PLAINTEXT_REMOTE` opt-in separately marks a trusted plaintext
LAN endpoint; endpoint URLs containing userinfo are always refused.
General `extra_body` fields are removed for artifact calls because guided
schemas, provider routing, beam search, and thinking overrides would add
authority or work outside the harness's measured reservation.

Token limits use Selfware's measured token counter. A call is not issued when
its prompt plus maximum reply would exceed the model context or the remaining
run token budget. A configured cost cap forces serial calls so the shared usage
ledger can stop before the next request. If a provider omits cost while a cost
cap is active, further calls stop and the result is partial. Token and cost
coverage, partial failures, format errors, timeouts, budget skips, and stale
bindings remain explicit in the report. `not_run`, `analysis_failed`,
`analysis_partial`, and `binding_changed` produce a nonzero process exit after
emitting the report.

If repository discovery reaches a file, entry, byte, or ranking-token ceiling,
or a walked source cannot be read and proved stable, the admitted set and
bounded error counts remain hash-bound. The run reports `analysis_partial` when
at least one artifact was analyzed, or `not_run` when none was available,
rather than claiming a complete selection.

## Current limits

Secure descriptor-relative artifact traversal is currently available on Unix.
On other platforms the command fails before repository discovery or model
egress; the ordinary checked file APIs retain their platform support.

This phase does not implement patch proposals, execution feedback, diagnosis of
test failures, dynamic activation outside the frozen set, or automatic
acceptance. Those require a shadow-worktree transaction, a fail-closed OS
sandbox, a verification manifest frozen before edits, and human review for
sensitive artifacts. The read-only boundary is the foundation for those later
phases, not an implicit authorization for them.
