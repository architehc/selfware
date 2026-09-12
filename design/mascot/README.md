# Phi — the Selfware fox

A mathematical interpretation of the existing fox in `src/ui/mascot.rs`.
The amber fur and copper details come from Selfware's Amber theme. The cream
muzzle and spiral tail give it a recognizable silhouette without lettering.

Open `index.html` directly in a browser. There are no dependencies, font
downloads, network calls, or build step. On this Mac:

```sh
open /Users/ivo/selfware/design/mascot/index.html
```

The page provides six expressions, three geometry controls, construction lines,
light/dark backgrounds, optional breathing/blinking, and SVG download.
The reduced-motion preference disables animation. Exported SVGs are static,
transparent, and contain their parameters in `<metadata>`.

## The mathematics

Coordinates below are mascot coordinates. The drawing maps them into the SVG
with `translate(247 304) scale(158)`; positive y points down.

### 1. A superellipse body

For `t ∈ [0, 2π]`, `a = 0.455`, `b = 0.52`, and `n ∈ [2, 3.2]`:

```text
x(t) = a · sign(cos t) · |cos t|^(2/n)
y(t) = 0.55 + b · sign(sin t) · |sin t|^(2/n)
```

Equivalently, `|x/a|ⁿ + |(y−0.55)/b|ⁿ = 1`. The default exponent is 2.4.
Changing the exponent rounds or squares the seated body while retaining symmetry.

### 2. A head with mirrored Gaussian ears

Let `c = cos t`, `s = sin t`, and `e ∈ [0.62, 1.04]`:

```text
G(z) = exp(−z² / 0.025)

x(t) = 0.66 c (1 + 0.32 s)
y(t) = −0.40 − 0.52 s − e max(s, 0) [G(c−0.68) + G(c+0.68)]
```

The two Gaussian peaks pull the upper contour into ears. The lower contour
tapers into a muzzle. Replacing `t` with `π−t` negates x and preserves y, so
the head stays exactly mirrored. The default ear parameter is 0.82.

### 3. A golden-spiral ribbon tail

Let `φ = (1+√5)/2`, `u ∈ [0,1]`, and `ω = 5.2 · curl`, where
`curl ∈ [0.78,1.18]`:

```text
r(u) = 0.98 · φ^(−2ωu/π)
θ(u) = 2.1 − ωu
C(u) = (0.75 + r(u) cos θ(u), 0.13 + r(u) sin θ(u))

N(u) = (−C′y(u), C′x(u)) / ||C′(u)||
w(u) = 0.018 + 0.205 · sin(πu)^0.7

tail edges = C(u) ± w(u) N(u)
```

Every quarter-turn reduces the centerline radius by exactly φ. The ribbon's
width is independently tapered, and its final 30% is cream. The default curl
is 1.0. This is an actual golden spiral, not a claim that every mascot proportion
follows the golden ratio.

### Finishing curves

The inner ears, muzzle, chest, nose, and expression lines use quadratic and
cubic Bézier curves. They are explicit paths in `mascot.js`; the entire fox
is not claimed to come from a single equation. Parametric outlines are sampled
into SVG paths at fixed resolution. Each export is deterministic for its
parameters and scales as a vector.

## States & Expressions

| Mood Key | Studio Label | Visual Motif | Mapped Selfware Event | Acoustic Signature |
| --- | --- | --- | --- | --- |
| `greeting` | Hello | Open, attentive eyes | `selfware init`, `boot` | Rising warm two-tone chime ($E_5 \to G^\#_5$) |
| `thinking` | Thinking | Contemplative eye, brow, 3 dots | `selfware chat`, planning | Dual-harmonic droplet ping ($A_4 \cdot \phi$) |
| `working` | Working | Focused narrowed eyes | `selfware run`, tool calls | Tactile code typewriter purr / tick |
| `success` | Bloom | Smiling curved eyes, green sprout | `cargo test --pass`, invariants ok | Ascending pentatonic bloom arpeggio |
| `error` | Concerned | Worried brows, downturned mouth | `clippy -D warnings`, test fail | Sympathetic minor descending interval |
| `idle` | Resting | Closed, relaxed eyes | Waiting for user input | Soft filtered pink noise breath / exhalation |
| `curious` | Inspect | Wide round eyes, specular dots, reticle | `selfware graph`, `analyze` | Upward frequency glide chirp ($440 \to 880\text{ Hz}$) |
| `evolve` | Evolve | Serene visionary gaze, golden $\phi$ glyph | `selfware evolve`, RSI loop | Golden ratio chord ($432\text{ Hz} \cdot [1, \phi, \phi^2]$) |
| `flow` | Flow | Aerodynamic slit eyes, speed streaks | `selfware multi-chat`, `--mode yolo` | High-throughput rhythmic keystroke blips |
| `guard` | Guarded | Watchful level gaze, trust shield | `selfware trust`, safety gate | Resonant metallic boundary latch click |
| `spark` | Eureka | Joyful grin, diamond starbursts | `swebench pro`, milestone pass | Crystalline high-register sparkle arpeggio |
| `sleep` | Dormant | Deep resting curves, drifting 'z's | Daemon suspended, low power | Sub-bass peaceful sine fade ($130.8\text{ Hz}$) |

## Procedural Web Audio Voice

In keeping with the project's zero-dependency philosophy, Phi's acoustic voice
uses the browser's native Web Audio API (`AudioContext`, `OscillatorNode`, `BiquadFilterNode`,
`GainNode`). There are zero audio files, downloads, or network requests.

Harmonic intervals follow the golden ratio $\phi = (1+\sqrt{5})/2 \approx 1.618034$ and
natural tuning (A4 = 432 Hz and 440 Hz), with exponential ADSR envelopes to prevent
clicks. Audio starts disabled by default to respect browser autoplay policies, and
can be enabled with the header toggle or the control switch.

## Accumulated State Embedding ($\mathbf{z} \in \mathbb{R}^6$)

Rather than treating moods as isolated instantaneous states, Phi maintains an
accumulated 6D continuous state embedding vector:

$$\mathbf{z} = [z_{\text{foc}}, z_{\text{vit}}, z_{\text{cla}}, z_{\text{cur}}, z_{\text{har}}, z_{\text{exp}}]^T$$

- $z_{\text{foc}}$ (**Focus** $\in [0, 1]$): Builds during consecutive tool turns; decays during prolonged idle.
- $z_{\text{vit}}$ (**Vitality** $\in [0, 1]$): Stamina reserves; drains on tool execution, recharges during sleep/idle.
- $z_{\text{cla}}$ (**Clarity** $\in [0, 1]$): Ratio of verified invariants and green tests to compile errors.
- $z_{\text{cur}}$ (**Curiosity** $\in [0, 1]$): AST exploration and graph traversal arousal.
- $z_{\text{har}}$ (**Harmony** $\in [0, 1]$): Proximity of mascot parameters to golden-ratio geometric balance.
- $z_{\text{exp}}$ (**Experience** $\ge 0$): Monotonically accumulating odometer of completed turns.

### State Transition & Posture Coupling

Every terminal event $\mathbf{e}$ updates the state via:

$$\mathbf{z}_{t+1}[0..4] = \text{clamp}\left(\mathbf{z}_t[0..4] + \mathbf{W} \cdot \mathbf{e}, 0, 1\right), \quad z_{\text{exp}} \leftarrow z_{\text{exp}} + \Delta_{\text{exp}}$$

When **Couple state embedding to posture** is enabled:
- Ear alertness $e$ is biased by curiosity ($z_{\text{cur}}$) and focus ($z_{\text{foc}}$).
- Tail curl tightness $c$ is biased by focus and vitality.
- Body softness $s$ relaxes when tired ($1 - z_{\text{vit}}$), yielding a heavier, rounded posture.
- Breathing period modulates dynamically: low vitality slows and deepens the breath cycle from 1.55s to 2.2s, while blink duration elongates.

### Archetype Cosine Classifier

Phi's normalized state is compared against archetype profiles using cosine similarity:

$$\cos \theta = \frac{\mathbf{z}_{0..4} \cdot \mathbf{w}_{\text{arch}}}{\|\mathbf{z}_{0..4}\| \|\mathbf{w}_{\text{arch}}\|}$$

Archetypes include:
- **The Architect**: High Focus, High Clarity, High Harmony
- **The Scout**: High Curiosity, High Vitality
- **The Sprinter**: High Focus, Lower Vitality, Fast Throughput
- **The Scribe**: Balanced Clarity, Documentation, and Care
- **The Sage**: High Experience, Golden-Ratio Alignment

The embedding state persists automatically in browser `localStorage`.

## Selfware Command Bridge

The interactive studio includes a command simulator where developers can type or click
Selfware commands (`selfware init`, `selfware run`, `selfware graph`, `selfware evolve`,
`cargo test`, `cargo clippy`, `selfware trust`, `swebench pro`, `daemon sleep`). The bridge routes
events to the corresponding expression, triggers the acoustic voice, mutates the state embedding,
and records the event in the telemetry feed.

## The shipped assistant lives elsewhere

This directory is the **authoring studio** for Phi's geometry and motion: it
generates the static brand vectors and is where new poses and animations are
worked out. It is not what ships.

The running assistant is `src/evolve/web/phi/` — which now draws **this** fox, with
CMU-dictionary lip sync, a laser focus beam, friction telemetry, and a local
VibeVoice speech stack. An earlier `assistant/` prototype lived here; every part
of it is now superseded by that directory, and its one unique contribution — a
procedural F1/F2 formant voice for machines with no installed voice pack — was
carried over as `src/evolve/web/phi/phi_formant.js`.

The studio's animation loop is the origin of the shipped rig's idle motion:
thoracic breathing, cervical counter-bob, the ear micro-twitch spring, gaze
relaxation and the Gaussian blink were all ported from here into
`src/evolve/web/phi/phi_rig.js`, and are now driven by that side's state vector.
The 12 moods below are the shared vocabulary — `scripts/tests/test_phi_expression.py`
fails if this list and `phi_expression.js` disagree.

**Resolved 2026-09-12:** the two no longer draw different foxes. This studio's
seated fox with its golden-spiral tail is the shipped character; the geometry
lives in `src/evolve/web/phi/phi_fox.js` and the mood vocabulary is shared and
enforced by `scripts/tests/test_phi_expression.py`. See `docs/PHI_ASSISTANT.md`
for the rationale.

## Files and reuse

- `mascot.js`: geometry, procedural audio, 6D state engine, and command simulator.
- `index.html`, `style.css`: the local interactive geometry studio.
- `motion/`: the Motion Studio — continuous-time kinematics, gestures, and the Fables of Phi narration.
- `selfware-phi.svg`: the complete mascot, transparent background.
- `selfware-phi-icon.svg`: face-only vector for small placements.
- `selfware-phi.png`, `selfware-phi-icon.png`: transparent PNG exports.
- `expressions/*.svg`: all 12 expression variants.
- `preview-*.png`, `preview-dynamic.jpg`: browser previews.
- `verification.json`: integrity check record with SHA-256 hashes.

Public browser API:

```javascript
SelfwareMascot.svg({
  ears: 0.82,
  curl: 1.0,
  softness: 2.4,
  mood: "evolve",
  construction: false,
});

SelfwareMascot.iconSvg({ mood: "curious" });

// Command simulator API
SelfwareMascot.executeCommand("selfware evolve");

// Acoustic voice API
SelfwareMascot.audio.enabled = true;
SelfwareMascot.audio.play("spark");

// State embedding vector
console.log(SelfwareMascot.stateEngine.vector);
console.log(SelfwareMascot.stateEngine.getArchetype());
```
