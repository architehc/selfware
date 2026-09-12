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
  mascot to its reserved perch. **God Mode** changes the artwork and tail
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

## The character changed on 2026-09-12

Phi was drawn twice: a cyber nine-tailed kitsune in the rig, and a seated
parametric fox in `design/mascot/`. The studio fox is now the shipped character,
and this is a **design decision, not a side effect of deduplication**:

- One drawing everywhere, so the brand and the assistant stop diverging.
- Its geometry is derived and parameterised (superellipse body, mirrored
  Gaussian ears, golden-spiral tail), so expression is a change of *parameter*
  rather than a swap of artwork — ears flatten by varying the Gaussian term,
  not by rotating a separate triangle off the silhouette.
- The warm amber palette reads as a companion. The kitsune read as a product.

Consequences to be aware of: `rig.tailElements` is now 2 paths (spiral ribbon +
cream tip) rather than 9 spline strokes, and `test_phi_rig.py` was updated to
match. Anything depending on nine tails is depending on the old character.

## Expressions and state

Phi has one expression vocabulary. `src/evolve/web/phi/phi_expression.js` holds the
12 canonical moods — greeting, thinking, working, success, error, idle, curious,
evolve, flow, guard, spark, sleep — as facial *parameters* (brow angle and lift,
eyelid coverage, pupil scale, ear rotation, resting mouth curve, tail energy, an
accent colour and an optional accessory). The rig interpolates toward them, so a
mood change is a movement rather than a swap.

`design/mascot/` draws the same 12 for the static brand vectors.
`scripts/tests/test_phi_expression.py` fails if either side gains or loses a mood,
which is what keeps the two from drifting apart again.

Operational names the friction companion already emits — `analytical`, `alert`,
`head_tilt`, `pacing`, `stretch`, `god_mode` — are aliases onto canonical moods.
An alias keeps its own status wording and accent, so "Loop Detected" never
silently becomes "In flow"; only the face is shared. An unrecognised name
resolves to `greeting` rather than throwing, because an emotion arriving from
telemetry must not be able to strand the face mid-render.

`phi_state.js` chooses the face. Rather than every caller picking an expression,
events nudge a six-axis vector — focus, vitality, clarity, curiosity, harmony and
a monotonic experience odometer — which relaxes toward rest between events. The
expression is read *off* that vector, so a long green streak and a single passing
test no longer look identical. An unambiguous event pins its face for ~2.6s, then
the vector takes over. A cosine classifier labels the working style (Architect,
Scout, Sprinter, Scribe, Sage); it is a label for the UI and gates nothing.

These axes are behavioural bookkeeping over events Selfware already emits. They
are not measurements of a model's internals and not claims about affect — they
drive an animation, and nothing reads them back as ground truth. Unknown events
are ignored rather than guessed at. Persistence via `localStorage` is
best-effort; a blocked or corrupt store leaves Phi working, just forgetful, and a
stale snapshot is relaxed by the time away instead of resuming mid-sprint.

### Idle motion

Phi's idle motion is ported from the `design/mascot/` studio loop, which had the
better model for looking alive:

- **Thoracic breathing.** The chest widens as it shortens rather than pulsing
  uniformly — an evenly scaled fox reads as a zooming sprite, not a breathing
  animal.
- **Cervical counter-bob.** The head counters the breath a beat late. That phase
  lag is most of what makes head and body read as one creature.
- **Ear micro-twitch.** A damped impulse spring flicks an ear every 4–9s, and
  never while asleep.
- **Gaze relaxation.** A target held for 3.5s is released and Phi settles
  front-on; a gaze held forever reads as a stare. Re-aiming at the same point
  does not renew attention.
- **Gaussian blink.** The lid closes on a curve instead of a linear window.

Breath rate and blink duration are driven by the state engine's `vitality` axis,
so a drained Phi visibly breathes slower and deeper and holds its blinks longer.
The springs integrate semi-implicitly with a clamped step, so the long frame a
background tab delivers on wake settles instead of diverging.

Everything above is suppressed under `prefers-reduced-motion`, including the gaze
idle clock — relaxing a stare over 3.5s is itself motion.

Open `/phi/gallery.html` against a running workspace to see all 12 rendered by the
real rig, and to drive the state vector by hand.

## Speech and synchronization

The default voice path selects an installed English voice with
`SpeechSynthesisVoice.localService === true`. Browsers without a local voice can
still provide a labelled silent reading, or the procedural formant voice below.
Audio-off cancels active audible speech.
Native word-boundary events correct the spoken-word clock; browsers that omit
these events use an explicitly approximate clock. Phonemes within a native
spoken word are approximate because the browser does not report their alignment.

### Procedural formant voice

`phi_formant.js` is the offline tier: a sawtooth glottal source through two
bandpass resonators tuned to the F1/F2 targets of each mouth shape. It needs no
model, no voice pack, no files and no network, so it is audible on a machine
where every other engine is unavailable.

It reproduces the **vowel colour and rhythm** of the narration. It does not
synthesize intelligible speech and is labelled as such in the voice menu — it is
an audible cue track, not a substitute for a real voice.

It is driven by the same CMU-derived viseme schedule that drives the mouth, so
sound and lips come from one timeline and cannot drift apart. The tier is opt-in:
select "Procedural formant voice" in the voice menu, or construct the engine with
`formantFallback: true` to make it replace the mute `local_voice_unavailable`
path. Left alone, a machine with no installed voice still gets silence.

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
not provide alignment. `scripts/phi_align.py` closes this gap when whisperX is
installed, and its `visemes_from_audio()` derives mouth shapes from the audio's
own RMS envelope and zero-crossing rate — an animation heuristic, not measured
phonemes, but one that tracks the real waveform when alignment has fallen back.
Native browser voices, the procedural formant voice, and labelled silent reading
remain available in the voice menu. An explicitly selected VibeVoice failure is shown
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
python3 scripts/tests/test_phi_expression.py -v
python3 scripts/tests/test_phi_align.py -v
python3 scripts/tests/test_phi_formant.py -v
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
