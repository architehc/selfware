# Phi: a reading companion for the Selfware workspace

Phi can open saved workspace files, read selected source aloud, explain cited
code, and accompany editing with a flying, articulated fox. The interface lives
at `/phi/` on the local Selfware Evolve server. It uses the same configured model,
workspace session, document hashes, checked writes, and grounded review service
as the main IDE.

## Start a workspace

```sh
cargo build --release --bin selfware
python3 scripts/run_phi_assistant.py \
  --binary target/release/selfware \
  --workdir /path/to/project \
  --config /absolute/path/to/selfware.toml
```

The launcher starts `selfware self-evolve`, waits for its local workspace and Phi
page to be ready, and then opens the browser. `--no-browser` keeps it headless;
`--startup-timeout` allows a longer initial graph scan on large projects.
For unusually large workspaces or debug builds, an explicit allowance such as
`--startup-timeout 900` permits a longer scan. Source inventory excludes
recognized Python virtual environments, including custom directory names,
before measured token counting.
`--demo` explicitly serves the illustrative example missions without a backend.
The launcher does not silently replace a failed workspace with a demo.

You can also run `selfware self-evolve --port 7777` and open
`http://127.0.0.1:7777/phi/`. The Phi button in Evolve links to the same interface.
No external JavaScript, fonts, speech SDK, or package installation is required
by the browser page.

## Work with Phi

- Choose a real file. **Edit** opens its buffer; **Save changes** uses the saved
  document hash and verifies the returned contents by reading them from disk.
  Concurrent changes produce a visible conflict and preserve the buffer.
- Select code text and choose **Read selection**, or press a line number.
  The spotlight follows real DOM text ranges, including word offsets when the
  narration is exactly the selected source. Scrolling remeasures the target.
- Ask a question and choose **Prepare a reading**. A separate grounded assistant
  request uses the saved source. The job survives page reloads; queued work,
  running inference, failure, and completion have distinct states. A transient
  polling failure retains the job identifier instead of launching another model
  request.
- **Super facts** show generated explanations, suggested improvements, and
  clickable source references. **Explain this** reads a fact while pointing at
  its cited code. An explicitly named line inside the citation or a unique exact
  code fragment can narrow the spotlight;
  otherwise the interface shows the inclusive cited line range. A citation
  checks the source snapshot and excerpt, not semantic truth. The page never
  equates citation validation with passing code tests.
- **Voice**, speed, pause, and stop control narration. **Summon Phi** returns the
  mascot to its reserved perch. **God Mode** changes the artwork and nine-tail
  animation and remains selected through reading and summoning. It is a visual
  mode, not a claim of AGI or successful verification.

The three example missions are preserved as explicitly illustrative source.
They neither inspect a Docker runtime nor establish security or cache isolation.

## Friction companion

Phi's companion panel reacts to coding events with short, written responses.
Classification and commentary run locally without an LLM request or speech
generation. Commentary never moves editor focus, starts narration, changes the
main mascot's reading pose, or applies/reverts code. Escape dismisses the current
message; the panel also offers snooze and an off switch. Late-night reminders
are off by default. Preferences are the only companion data saved in browser
storage; events and diagnostics are not persisted there.

The event sources have different limits:

| Signal | Current source | What it establishes |
| --- | --- | --- |
| Generated diff size | Evolve's staged apply registry | Actual added/deleted lines, file count and diff digest |
| Compiler diagnostics | Completed Cargo analysis and bounded apply failure metadata | An observed error, with completeness reported; an unresolved symbol alone does not prove an invented API |
| Repeated review rejection | Explicit **Reject diff** in the staged review | The developer rejected distinct generated diffs; compiler refusals and closing a preview are separate events |
| Review time | Visible, focused diff preview during recent interaction | Measured active viewing intervals, excluding hidden/idle/suspended time; not a measure of cognitive effort |
| Undo | Monaco's semantic undo event or textarea `historyUndo` | An undo occurred; the built-in editor does not infer that a nearby agent generation caused it |
| Long-session reminder | Opt-in, bounded activity intervals, local hour and recent linked friction | A combination of observed events; not a fatigue diagnosis |

The classifier requires task and generation correlation for unresolved-API,
oscillation and undo interventions. Alternating failures require four distinct
generations, A/B/A/B; polling the same failed run cannot multiply failures.
An oversized diff is described by its measured size, without guessing how many
lines the task should have needed. Rejection retains the staged worktree for
inspection and deliberate later Apply; it does not delete or roll back work.

`GET /api/friction/events?after=N` returns `{events,cursor,reset,capabilities}`.
Responses page through retained events with a bounded payload; use the returned
cursor for the next request, including when catching up after a gap.
`POST /api/friction/events` accepts `{events:[...]}` from explicit IDE adapters.
Both require the workspace `x-selfware-session` header. The backend retains at
most 256 events for one hour in process memory, accepts at most 32 events or
32 KiB per POST, and validates typed fields. It rejects raw code, paths, prompts,
compiler output and client claims of server provenance. Event IDs support
bounded deduplication. The browser also bounds and expires its classifier
history. A feed gap resets classification instead of treating missing events as
an unbroken failure sequence.

Example adapter event, using an opaque task/generation already observed by the
feed (independent adapters report their own generation first):

```json
{"events":[{"id":"review-unique-1","kind":"review_closed","task_id":"task-opaque-1","generation_id":"run-opaque-1","data":{"decision":"rejected","added_lines":400,"active_review_ms":45000}}]}
```

The browser labels simulation separately and keeps simulated events out of the
live feed. The off switch pauses commentary and editor reporting across tabs on
the same origin; server observations may continue in the bounded local ring.
Snooze pauses nudges. No LSP subscription, npm package-index verification,
biometric input or local language-model commentary is implemented by this path.
Those require explicit adapters or a later model integration. The older
`phi_friction.js` prototype is retained separately; production uses
`phi_friction_monitor.js` and `phi_friction_ui.js`.

Today, built-in hooks can trigger live large-diff and repeated-review-rejection
nudges. Unresolved-API and alternating-error nudges require complete,
generation-linked diagnostic reports from an explicit adapter. Standalone
Cargo checks lack that link, and retained apply failure excerpts are marked
incomplete, so neither is promoted to those claims. Rapid-undo nudges likewise
require an adapter that knows which generated patch was actually applied to the
edited document. The current diagnostic ingress accepts Rust toolchains/codes;
TypeScript/Python detection in the classifier is not a connected LSP service.
Simulation exercises these rules without asserting that missing hooks exist.

## The motion model

Phi keeps one SVG rig and changes its pose numerically. Flight uses a critically
damped spring on each axis. For one bounded animation step, with target `g`,
position error `e = x - g`, velocity `v`, and response rate `w`:

```text
x_next = g + (e + (v + w*e)*dt) * exp(-w*dt)
v_next = (v - w*(v + w*e)*dt) * exp(-w*dt)
```

Changing the destination preserves the current position and velocity. The ten
mouth postures share the same control-point layout and blend with
`1 - exp(-32*dt)`, so an interrupted phoneme continues from the visible mouth.
Nine cubic Bézier tails combine phase-shifted sine waves with drag from flight
velocity. Eye direction and the pointing beam use the rendered SVG transform,
so their origin follows the moving head. Reduced motion keeps a readable static
pose and suppresses particles. These are animation parameters, not measurements
of physical anatomy or audio amplitude.

## Speech and synchronization

The default voice path selects an installed English voice with
`SpeechSynthesisVoice.localService === true`. Browsers without a local voice can
still provide a labelled silent reading. Audio-off cancels active audible speech.
Native word-boundary events correct the spoken-word clock; browsers that omit
these events use an explicitly approximate clock. Phonemes within a native
spoken word are approximate because the browser does not report their alignment.

Before native synthesis, `<`, `>` and `&` are narrated as "less than", "greater
than" and "ampersand". This keeps code delimiters literal in an API that also
accepts [SSML input](https://webaudio.github.io/web-speech-api/#dom-speechsynthesisutterance-text).
The displayed source or explanation is unchanged. A UTF-16 offset map translates
native speech boundaries back to the original text for captions and source
highlights; source reading and generated explanations share this path.

The `nativeCompletion` receipt records whether a final-word boundary was
observed, only partial word boundaries were available, or just the native end
event arrived. Sparse boundary callbacks do not cause a false failure, and a
native end event alone is not presented as proof of complete word coverage.

English pronunciation uses a bundled CMU Pronouncing Dictionary with 125,535
unique entries, a pinned upstream revision, SHA-256 provenance, and its license
in `src/evolve/web/phi/assets/`. Selfware/Phi pronunciations have explicit entries;
unrecognized names and code identifiers use fallback rules. Dictionary coverage
does not establish accent-specific pronunciation or phoneme-level audio alignment.
Ten mouth postures interpolate continuously, including interrupted transitions.

## VibeVoice-Realtime-0.5B ONNX Speech Engine

Phi optionally runs [VibeVoice-Realtime-0.5B-ONNX](https://huggingface.co/elbruno/VibeVoice-Realtime-0.5B-ONNX)
in a separate local Python worker. The output is 24 kHz, mono, 16-bit PCM WAV.
Six English presets are available: **Emma** (default), Grace, Carter, Davis,
Frank, and Mike. This path uses the seven exported graphs and voice KV caches;
it does not use the obsolete three-graph example or synthesize substitute tones.

Install once with Python 3.12 or newer, then launch the owned worker and backend:

```sh
python3 scripts/setup_phi_vibevoice.py --python /path/to/python3.12
python3 scripts/run_phi_assistant.py \
  --binary target/release/selfware \
  --workdir /path/to/project \
  --config /absolute/path/to/selfware.toml \
  --speech vibevoice
```

Setup creates an isolated runtime under `~/.cache/selfware/vibevoice/` and pins
model revision `6825ea6fd389843b39a33d5a088c5993bf4fab4e`. Its 577 selected files
total **4,774,199,296 logical bytes (4.77 GB)**; the runtime needs additional disk
space, while the Hugging Face cache can share identical weight blobs. Setup
checks completeness, sizes, LFS SHA-256 hashes and regular Git blob hashes before
atomically publishing `installation.json`, which records every file's SHA-256.
Model installation is explicit: choosing a voice never downloads weights.

For another cache, pass `--cache-dir DIR` to setup and `--speech-cache DIR` to the
launcher. `--speech-python PATH` and `--speech-model-dir DIR` override the receipt's
paths. The launcher waits for the worker's bind marker and authenticated ready
status through Rust before opening Phi. Both processes stop on launcher shutdown
or startup failure. Static `--demo` does not start this service.

The current CPU path buffers the complete waveform before playback. On this
Apple Silicon Mac, a four-thread Emma probe produced **4.00 seconds of audio in
11.25 seconds**, after **12.60 seconds** of model loading. This is one measured
utterance, not a general throughput guarantee or streaming latency claim. Its
WAV, timing receipt, and independent local ASR check are retained under
`artifacts/phi-vibevoice-20260910/`; ASR recognized the sentence with a contraction
difference and does not establish human-rated voice quality.

The worker supplies **no measured word or phoneme alignment**. Phi follows
`audio.currentTime` and maps an explicitly approximate mouth/caption timeline
across the measured audio duration. Voice speed changes playback speed; it does
not provide alignment. Native browser voices and labelled silent reading remain
available in the voice menu. An explicitly selected VibeVoice failure is shown
as a failure; the UI does not silently change voices. With the launcher default
`--speech native`, no ONNX worker is started.

All five `/api/speech` routes require the workspace session header. The Rust
bridge uses a separate private bearer token to reach the worker's loopback
socket; browser audio is fetched with authentication, checked against its SHA-256
receipt, and played from a Blob URL. Requests have a 5,000-character ceiling,
with at most eight queued/running jobs and one active synthesis. WAV responses
are limited to 32 MiB. Finished jobs expire after 15 minutes; cache retention is
bounded to 64 jobs and 64 MiB of completed audio. Worker defaults are 450 frames
and 180 seconds per synthesis; explicit bounds cannot exceed 900 frames or
600 seconds. Hitting a limit reports incomplete synthesis rather than success.

Stop, mute, and a replacement reading cancel pending synthesis as well as
playback. A stable request ID makes repeated submissions idempotent; reusing an
ID for different text returns a conflict. Cancellation receipts remain for
15 minutes so a delayed submission cannot restart stopped speech. Cache or queue
pressure returns a visible error instead of discarding those cancellation records.

For another audio producer that supplies real alignment, the reusable adapter
accepts word/phoneme timestamps and follows pauses and seeks. The numbers below
illustrate that API; they are not VibeVoice output:

```javascript
await phiApp.viseme.speakAudio({
  audio: audioElement, // Or url: a same-origin audio URL / local Blob URL.
  text: narration,
  words: [{ start: 0.12, end: 0.48, charStart: 0, charEnd: 5 }],
  phonemes: [{ start: 0.12, end: 0.20, viseme: 'fv' }],
}, {
  onWord(word, characterOffset, timing) {
    // Update captions or a source range using the supplied real offsets.
  },
});
```

## Validation

Browser suites execute the production modules with controlled API/model fixtures:

```sh
python3 scripts/tests/test_vibevoice_tts.py -v
python3 scripts/tests/test_phi_workspace.py -v
python3 scripts/tests/test_phi_workspace_races.py -v
python3 scripts/tests/test_phi_citation_focus.py -v
python3 scripts/tests/test_phi_rig.py -v
python3 scripts/tests/test_phi_speech_focus.py -v
python3 scripts/tests/test_phi_mobile_focus.py -v
python3 scripts/tests/test_phi_launcher.py -v
python3 scripts/tests/test_phi_vibevoice_setup.py -v
python3 scripts/tests/test_phi_vibevoice_runtime.py -v
python3 scripts/tests/test_phi_speech_worker.py -v
python3 scripts/tests/test_phi_speech_client.py -v
python3 scripts/tests/test_phi_local_speech.py -v
python3 scripts/tests/test_phi_voice.py -v
```

Worker/runtime tests use the installed speech Python environment. The setup and
launcher tests use only the Python standard library and synthetic fixtures.

They cover persisted model jobs, stale citations, selected text offsets, checked
saves, delayed-load/citation/save races, source scrolling, speech cancellation,
voice gating, audio timelines, pronunciation coverage, mouth morphs, reduced
motion, flight, and launcher ownership. These are not evidence of live model
inference or a human judgment of voice quality; those require separate checks.
Rust protocol and embedded-asset tests cover the actual service contracts.

The real ONNX integration was also exercised in Chrome on 2026-09-10 through
the authenticated Rust bridge and the actual mission reader. Thirteen checks
passed, covering the selected voice, measured media clock, pause/resume,
generation cancellation, mute, WAV hash validation, and terminal Blob cleanup.
The mouth probe uses the rig's connected SVG element; replaying the same real
WAV recorded 91 distinct rendered mouth paths, without another model request.
The initial incorrect document-level selector and its correction are retained
in `artifacts/phi-vibevoice-20260910/live-browser-verification.json` and
`live-mouth-recheck.json`. The delivered WAV is byte-identical to the independently
transcribed runtime sample. These checks establish playback and animation
behavior, not exact phoneme alignment or a human assessment of voice quality.

A separate 40-word paragraph consumed all 46 model tokens and completed at EOS:
15.6 seconds of audio, generated in 25.06 seconds during a concurrent release
build. Local ASR matched 39 words and rendered the initial name "Phi" as "Fee";
the strict exact-match result remains false. The measured WAV, job telemetry,
and transcription comparison are retained as `paragraph-*` in the same folder.

A live integration run on 2026-09-10 used the configured
`llm.selfware.design` endpoint (`qwen38-flash-next`) against a small real
workspace. It produced four cited claims and one recommendation using 1,394
API-reported tokens. Replaying that captured result checks exact anchors at
the function declaration, input guard, and return expression without another
model request. Reports and browser captures are under
`artifacts/phi-assistant-live-20260910/`. Citation integrity is structural;
the generated numerical recommendations were not semantically verified.

A separate local Daniel speech probe on the same date reproduced a premature
native end in a 165-character paragraph containing `Option<f64>`: its last
boundary was `Option`, before the rest of the paragraph. Through the updated
Phi engine, the unchanged original paragraph reached its final `unusable.`
boundary at original offset 156 and returned
`nativeCompletion.finalWordBoundaryReached: true`. The probe also checked that
displayed text and mapped offsets were preserved. Native output was muted;
this verifies callback coverage, not recorded audio quality or phoneme alignment.
The original comparison and six passing engine checks are retained in
`artifacts/phi-native-speech-20260910/`.
