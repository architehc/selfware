# Phi in motion

A coordinated vector character for Selfware: twelve expressions, six gestures,
pointer attention, and smooth transitions. This preview preserves the original
Phi fox's amber palette, Gaussian ears, superellipse body, and spiral tail.

Open `index.html` directly in a browser. The initial showreel changes scenes
every eight seconds of animation time. Selecting an expression or gesture
switches to manual control. Tempo, pause, and pointer attention are independent.
No package install, server, external fonts, or image API is needed.

## Motion language

Eyes lead; the head follows; ears and tail settle later. Greeting waves twice
and rests. Thinking lifts a paw and looks aside. Working alternates short paw
taps with pauses. Bloom raises both paws once. Concern grows still. Resting
breathes quietly. Curiosity, evolution, flow, guarded attention, a new idea, and
sleep share the same underlying character.

The six additional gestures are wave, look around, stretch, little walk,
celebrate, and nod. Paws use attached Bézier limbs, hind feet articulate during
walking, head proportions stay stable during body stretch, and the tail rotates
around its root. Ears deform the head contour instead of floating separately.

The preview's expression labels are simulated design states. They do not report
live Selfware task outcomes or imply that any checks have run.

## Why transitions stay smooth

Each pose channel has a critically damped spring with preserved position and
velocity. For target `g`, position `x`, velocity `v`, response `ω`, and timestep
`dt`, the exact update for a constant target over that timestep is:

```text
y = x − g
c = v + ωy
decay = exp(−ωdt)

x_next = g + (y + c·dt)·decay
v_next = (v − ωc·dt)·decay
```

Gaze responds faster than the head; the tail responds more slowly. Gestures
have eased entrances and exits. Blinks are short and sparsely scheduled.
The animation uses `requestAnimationFrame` without rebuilding the SVG each
frame. Hidden documents stop requesting frames. Reduced motion displays an
expressive still pose; gestures can be previewed as still poses too.

## Reuse as Selfware's face

Load these three dependency-free scripts, in this order:

```html
<script src="geometry.js"></script>
<script src="rig.js"></script>
<script src="motion.js"></script>
<div id="selfware-face"></div>
<script>
  const fox = PhiMotion.mount(document.getElementById("selfware-face"), {
    showreel: false,
  });
  fox.setState("thinking");
  // Later, when the application begins an operation:
  fox.setState("working");
  // Only after the application's required verification actually succeeds:
  // fox.setState("success");
  // Before removing the component:
  // fox.destroy();
</script>
```

`studio.js` is the separate preview UI and is unnecessary in an integration.
Use typed application events to choose states. In particular, compilation
alone must not trigger a verified-success celebration when required tests
remain failed or unrun.

Public controls:

```javascript
fox.setState("curious");
fox.gesture("wave");
fox.setTempo(0.85);              // Supported range: 0.5–1.5
fox.setAttention(true);
fox.setAnimation(false);         // Freeze the current pose
fox.setShowreel(false);
const still = fox.snapshot();    // SVG of the actual current pose
const loop = await fox.animatedSVG();
const avatar = await PhiMotion.iconLoopSVG("thinking", 1);
```

Supported states: `greeting`, `curious`, `thinking`, `working`, `success`,
`error`, `idle`, `evolve`, `flow`, `guard`, `spark`, `sleep`.
Supported gestures: `wave`, `look`, `stretch`, `walk`, `celebrate`, `nod`.
The controller dispatches a `phi:state` event on its mounting element when a
state, gesture, or control changes. Pointer input affects pose only.

## Artifacts

- `selfware-phi-animated.svg`: a standalone greeting/wave loop.
- `selfware-phi-avatar-animated.svg`: a compact thinking face for headers.
- `geometry.js`: the original mascot paths and their source SVG fingerprint.
- `rig.js`: attached body parts, deformable ears, eyes, limbs, and rendering.
- `motion.js`: choreography, spring controller, lifecycle, and export.
- `studio.js`, `style.css`, `index.html`: the interactive motion preview.

Standalone animated SVGs contain a deterministic six-second loop at normal
tempo, sampled into 121 vector keyframes with interpolation between them.
They use CSS transform animation and SVG shape animation; no JavaScript is
embedded. A separate static drawing is shown for reduced motion. The exported
loop follows the selected state and optional gesture, without pointer input;
the live interactive controller additionally uses spring dynamics.

The original `../index.html` studio is being developed separately and is left
intact. This module is an additive motion prototype, not a change to the Rust
terminal mascot or a connection to a live agent process.
