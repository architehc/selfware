/* Phi's geometry — the parametric fox from design/mascot, as the shipped character.
 *
 * Phi was drawn twice: a seated parametric fox in the studio, and a cyber
 * kitsune in the rig. The studio's fox is the character; this module is its
 * geometry, ported so one drawing ships everywhere.
 *
 * Three curves carry the silhouette (see design/mascot/README.md for the
 * derivations):
 *   body  a superellipse  |x/a|^n + |(y-.55)/b|^n = 1
 *   head  a tapered oval with two mirrored Gaussian ear peaks
 *   tail  a golden spiral whose radius falls by phi every quarter turn
 *
 * Everything else — inner ears, muzzle, chest, nose — is explicit Bezier work.
 * The fox is NOT claimed to come from a single equation.
 *
 * Coordinates here are mascot units, exactly as the studio writes them. The rig
 * maps them into its 200x200 viewBox with translate(ORIGIN) scale(SCALE), so
 * the art can be edited against the studio's own maths while the rig keeps
 * doing its pixel arithmetic for eyes, lids and visemes.
 */

export const PHI = (1 + Math.sqrt(5)) / 2;

// Mascot units -> the rig's 200x200 viewBox.
export const ORIGIN_X = 100;
export const ORIGIN_Y = 110;
export const SCALE = 76;
export const toScreen = (x, y) => [ORIGIN_X + x * SCALE, ORIGIN_Y + y * SCALE];

export const PALETTE = Object.freeze({
  fur: '#D4A373', tail: '#B87333', cream: '#FFF2DF', ink: '#241B16', sage: '#8F9779',
  shade: '#B98A5E', spark: '#E8C468'
});

// Supported parameter ranges, from the studio. Outside these the silhouette
// stops reading as the same animal.
export const RANGE = Object.freeze({
  ears: { min: .62, max: 1.04, rest: .82 },
  curl: { min: .78, max: 1.18, rest: 1 },
  softness: { min: 2, max: 3.2, rest: 2.4 }
});

const round = n => (Math.abs(n) < .000005 ? 0 : n).toFixed(4);
const point = p => p.map(round).join(' ');
const poly = (points, close = true) => 'M ' + points.map(point).join(' L ') + (close ? ' Z' : '');
const sample = (fn, start, end, count) =>
  Array.from({ length: count + 1 }, (_, i) => fn(start + (end - start) * i / count));
const signedPower = (x, p) => Math.sign(x) * Math.abs(x) ** p;

/* 1. Superellipse body, translated to the seated centre. */
export function body(t, n) {
  return [.455 * signedPower(Math.cos(t), 2 / n), .55 + .52 * signedPower(Math.sin(t), 2 / n)];
}

/* 2. Tapered oval with two Gaussian ear peaks, exactly mirrored about x=0.
 * Replacing t with pi-t negates x and preserves y, so the head cannot go
 * asymmetric no matter what `ears` is. */
export function head(t, ears) {
  const c = Math.cos(t), s = Math.sin(t);
  const peaks = Math.exp(-((c - .68) ** 2) / .025) + Math.exp(-((c + .68) ** 2) / .025);
  return [.66 * c * (1 + .32 * s), -.40 - .52 * s - ears * Math.max(s, 0) * peaks];
}

/* 3. Inward golden spiral. Every quarter turn reduces the centreline radius by
 * exactly phi; the ribbon's width is tapered independently. */
export function tail(u, curl) {
  const omega = 5.2 * curl, theta = 2.1 - omega * u;
  const radius = .98 * PHI ** (-2 * omega * u / Math.PI);
  const growth = 2 * omega * Math.log(PHI) / Math.PI;
  const x = .75 + radius * Math.cos(theta), y = .13 + radius * Math.sin(theta);
  const dx = radius * (-growth * Math.cos(theta) + omega * Math.sin(theta));
  const dy = radius * (-growth * Math.sin(theta) - omega * Math.cos(theta));
  const length = Math.hypot(dx, dy) || 1;
  const width = .018 + .205 * Math.sin(Math.PI * u) ** .7;
  return { x, y, nx: -dy / length, ny: dx / length, width };
}

export function bodyPath(softness, steps = 150) {
  return poly(sample(t => body(t, softness), 0, Math.PI * 2, steps));
}

export function headPath(ears, steps = 260) {
  return poly(sample(t => head(t, ears), 0, Math.PI * 2, steps));
}

export function tailRibbon(curl, start = 0, end = 1, steps = 130) {
  const coords = sample(u => tail(u, curl), start, end, steps);
  return poly([
    ...coords.map(p => [p.x + p.nx * p.width, p.y + p.ny * p.width]),
    ...coords.reverse().map(p => [p.x - p.nx * p.width, p.y - p.ny * p.width])
  ]);
}

/* Inner-ear patch, anchored to wherever the Gaussian peak currently sits so it
 * tracks the outline instead of sliding out of the ear as `ears` changes. */
export function innerEarPath(ears) {
  const [, peakY] = head(Math.acos(.68), ears);
  return `M .415 ${round(peakY + .14)} Q .65 -.84 .57 -.69 Q .46 -.73 .415 -.855 Z`;
}

const CHEST = 'M 0 .075 C -.19 .265 -.27 .565 0 .885 C .27 .565 .19 .265 0 .075 Z';
const MUZZLE = 'M 0 .109 C -.145 .105 -.495 -.06 -.586 -.338 C -.445 -.376 -.255 -.321 0 -.046 C .255 -.321 .445 -.376 .586 -.338 C .495 -.06 .145 .105 0 .109 Z';
const NOSE = 'M -.048 -.052 Q 0 -.069 .048 -.052 Q .042 -.025 0 -.007 Q -.042 -.025 -.048 -.052 Z';
const HAUNCH = 'M -.225 .45 Q -.2 .71 -.215 .93 M .225 .45 Q .2 .71 .215 .93';
const TOES = 'M -.24 1.014 Q -.18 .991 -.11 1.031 M .11 1.031 Q .18 .991 .24 1.014';

// Where the tail hinges when it sways, in screen units.
export const TAIL_PIVOT = toScreen(.22, .58);
// Eyes, muzzle and the neck the head rotates around, in screen units.
export const EYE_LEFT = toScreen(-.22, -.435);
export const EYE_RIGHT = toScreen(.22, -.435);
export const MOUTH_CENTRE = toScreen(0, .028);
export const NECK = toScreen(0, .09);

const unit = `translate(${ORIGIN_X} ${ORIGIN_Y}) scale(${SCALE})`;

/* The full character. Art is emitted in mascot units inside scaled groups; the
 * interactive face (eyes, lids, brows, visemes) stays in screen units so the
 * rig's pixel arithmetic keeps working unchanged. */
export function buildFox({ ears = RANGE.ears.rest, curl = RANGE.curl.rest, softness = RANGE.softness.rest } = {}) {
  const [eyeLX, eyeLY] = EYE_LEFT, [eyeRX, eyeRY] = EYE_RIGHT;
  const [mouthX, mouthY] = MOUTH_CENTRE;
  return `
    <defs>
      <linearGradient id="phi-fur-grad" x1="12%" y1="0%" x2="88%" y2="100%">
        <stop offset="0%" stop-color="#E0B68C" /><stop offset="55%" stop-color="${PALETTE.fur}" />
        <stop offset="100%" stop-color="${PALETTE.shade}" />
      </linearGradient>
      <linearGradient id="phi-tail-grad" x1="0%" y1="0%" x2="100%" y2="100%">
        <stop offset="0%" stop-color="#C98545" /><stop offset="100%" stop-color="${PALETTE.tail}" />
      </linearGradient>
      <radialGradient id="phi-eye-amber" cx="38%" cy="34%" r="66%">
        <stop offset="0%" stop-color="#8A6A46" /><stop offset="100%" stop-color="${PALETTE.ink}" />
      </radialGradient>
      <filter id="phi-glow" x="-50%" y="-50%" width="200%" height="200%">
        <feGaussianBlur in="SourceGraphic" stdDeviation="3" result="b" />
        <feMerge><feMergeNode in="b" /><feMergeNode in="SourceGraphic" /></feMerge>
      </filter>
      <filter id="phi-god-glow" x="-60%" y="-60%" width="220%" height="220%">
        <feGaussianBlur in="SourceGraphic" stdDeviation="5" result="b1" />
        <feGaussianBlur in="SourceGraphic" stdDeviation="13" result="b2" />
        <feMerge><feMergeNode in="b2" /><feMergeNode in="b1" /><feMergeNode in="SourceGraphic" /></feMerge>
      </filter>
    </defs>

    <g id="phi-god-rings" opacity="0" class="phi-god-layer">
      <circle cx="100" cy="104" r="84" fill="none" stroke="${PALETTE.spark}" stroke-width="1.4" stroke-dasharray="6,4,18,4" />
      <circle cx="100" cy="104" r="93" fill="none" stroke="${PALETTE.sage}" stroke-width="1" stroke-dasharray="20,8,4,8" opacity=".75" />
      <polygon points="100,16 182,104 100,192 18,104" fill="none" stroke="rgba(232,196,104,.28)" stroke-width="1" />
    </g>

    <g id="phi-body-group">
      <g id="phi-tails-group">
        <g transform="${unit}">
          <path id="phi-tail-ribbon" d="${tailRibbon(curl)}" fill="url(#phi-tail-grad)" />
          <path id="phi-tail-tip" d="${tailRibbon(curl, .70)}" fill="${PALETTE.cream}" />
        </g>
      </g>

      <g transform="${unit}">
        <path id="phi-body-shape" d="${bodyPath(softness)}" fill="url(#phi-fur-grad)" />
        <path d="${CHEST}" fill="${PALETTE.cream}" />
        <path d="${HAUNCH}" fill="none" stroke="${PALETTE.tail}" stroke-width=".012" stroke-linecap="round" opacity=".7" />
        <path d="${TOES}" fill="none" stroke="${PALETTE.tail}" stroke-width=".012" stroke-linecap="round" />
      </g>

      <g id="phi-head">
        <g transform="${unit}">
          <path id="phi-head-shape" d="${headPath(ears)}" fill="url(#phi-fur-grad)" />
          <g id="phi-ear-left"><path id="phi-inner-ear-left" d="${innerEarPath(ears)}" transform="scale(-1 1)" fill="${PALETTE.tail}" /></g>
          <g id="phi-ear-right"><path id="phi-inner-ear-right" d="${innerEarPath(ears)}" fill="${PALETTE.tail}" /></g>
          <path d="${MUZZLE}" fill="${PALETTE.cream}" />
          <path d="${NOSE}" fill="${PALETTE.ink}" />
        </g>

        <!-- Audio level, read inside the ears rather than across the cheeks. -->
        <g id="phi-ear-eq-left" opacity=".8">
          <rect x="57.5" y="47" width="3.6" height="1.8" rx=".9" fill="${PALETTE.cream}" />
          <rect x="58.5" y="51" width="3.6" height="1.8" rx=".9" fill="${PALETTE.cream}" />
          <rect x="59.5" y="55" width="3.6" height="1.8" rx=".9" fill="${PALETTE.spark}" />
        </g>
        <g id="phi-ear-eq-right" opacity=".8">
          <rect x="138.9" y="47" width="3.6" height="1.8" rx=".9" fill="${PALETTE.cream}" />
          <rect x="137.9" y="51" width="3.6" height="1.8" rx=".9" fill="${PALETTE.cream}" />
          <rect x="136.9" y="55" width="3.6" height="1.8" rx=".9" fill="${PALETTE.spark}" />
        </g>

        <g id="phi-brows" stroke="${PALETTE.ink}" stroke-width="2" stroke-linecap="round" fill="none">
          <path id="phi-brow-left" d="M ${eyeLX - 7} ${eyeLY - 7.5} Q ${eyeLX} ${eyeLY - 11} ${eyeLX + 7} ${eyeLY - 7.5}" />
          <path id="phi-brow-right" d="M ${eyeRX - 7} ${eyeRY - 7.5} Q ${eyeRX} ${eyeRY - 11} ${eyeRX + 7} ${eyeLY - 7.5}" />
        </g>

        <!-- This fox's eyes are dark almond pupils, as the studio draws them:
             no sclera ring and no iris disc. Adding either makes them bulge. -->
        <g id="phi-eye-left-group">
          <g id="phi-pupils-left">
            <ellipse id="phi-pupil-left" cx="${eyeLX}" cy="${eyeLY}" rx="2.6" ry="4.2" fill="${PALETTE.ink}" />
            <circle cx="${eyeLX + 1.05}" cy="${eyeLY - 1.7}" r=".85" fill="${PALETTE.cream}" opacity=".92" />
          </g>
          <rect id="phi-eyelid-left" x="${eyeLX - 5.2}" y="${eyeLY - 6.4}" width="10.4" height="0" rx="2.4" fill="url(#phi-fur-grad)" />
        </g>
        <g id="phi-eye-right-group">
          <g id="phi-pupils-right">
            <ellipse id="phi-pupil-right" cx="${eyeRX}" cy="${eyeRY}" rx="2.6" ry="4.2" fill="${PALETTE.ink}" />
            <circle cx="${eyeRX + 1.05}" cy="${eyeRY - 1.7}" r=".85" fill="${PALETTE.cream}" opacity=".92" />
          </g>
          <rect id="phi-eyelid-right" x="${eyeRX - 5.2}" y="${eyeRY - 6.4}" width="10.4" height="0" rx="2.4" fill="url(#phi-fur-grad)" />
        </g>

        <g id="phi-eye-arcs" stroke="${PALETTE.ink}" stroke-width="1.9" stroke-linecap="round" fill="none" opacity="0">
          <path d="M ${eyeLX - 4.6} ${eyeLY + 2} Q ${eyeLX} ${eyeLY - 4.6} ${eyeLX + 4.6} ${eyeLY + 2}" />
          <path d="M ${eyeRX - 4.6} ${eyeRY + 2} Q ${eyeRX} ${eyeRY - 4.6} ${eyeRX + 4.6} ${eyeRY + 2}" />
        </g>

        <g id="phi-mouth-group" transform="translate(${round(mouthX - 100)} ${round(mouthY - 103)})">
          <path id="phi-mouth-cavity" d="M 92 103 Q 100 103 108 103 Q 100 103 92 103 Z" fill="#5A2E22" />
          <path id="phi-mouth-tongue" d="M 96 103 Q 100 101 104 103 Q 100 104 96 103 Z" fill="#C97B6B" opacity="0" />
          <path id="phi-mouth-teeth" d="M 94 102 L 106 102" stroke="${PALETTE.cream}" stroke-width="1.6" stroke-linecap="round" opacity="0" />
          <path id="phi-mouth-lip" d="M 92 103 Q 100 104 108 103" fill="none" stroke="${PALETTE.ink}" stroke-width="1.5" stroke-linecap="round" />
        </g>

        <g id="phi-accessory" opacity="0"></g>
      </g>
    </g>
  `;
}
