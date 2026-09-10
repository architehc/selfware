/**
 * Selfware Mascot Assistant: "Phi the Fox" (Φ)
 * Spatial Code Focus & Optical Targeting Coordinator
 *
 * Implements:
 * - Line-level, token-level, and DOM bounding-box coordinate resolution
 * - Non-occluding spatial parking planner (parks Phi adjacent to code without blocking)
 * - Cubic Bézier / spring flight trajectory execution
 * - Optical laser targeting & dynamic code bracket highlight
 * - Smooth viewport auto-scrolling to keep focused code centered
 * - Gaze vector alignment (head & pupil tracking of target point)
 */

export class PhiFocusCoordinator {
  constructor(rig, visemeEngine, editorContainer) {
    this.rig = rig;
    this.viseme = visemeEngine;
    this.editorContainer = editorContainer;
    this.activeHighlightEl = null;
    this.highlightBracketEl = null;
    this.initDOM();
  }

  initDOM() {
    // Dynamic overlay for glowing code spotlight & brackets
    this.spotlightOverlay = document.createElement('div');
    this.spotlightOverlay.className = 'phi-code-spotlight';
    this.spotlightOverlay.style.position = 'absolute';
    this.spotlightOverlay.style.pointerEvents = 'none';
    this.spotlightOverlay.style.display = 'none';
    this.spotlightOverlay.style.zIndex = '50';
    this.spotlightOverlay.style.transition = 'all 0.25s cubic-bezier(0.16, 1, 0.3, 1)';
    this.spotlightOverlay.innerHTML = `
      <div class="phi-bracket-left">◀</div>
      <div class="phi-spotlight-beam"></div>
      <div class="phi-bracket-right">▶</div>
    `;
    document.body.appendChild(this.spotlightOverlay);
  }

  // Resolves client coordinates for a given code line number
  resolveLineCoordinates(lineNumber) {
    // Look for line element with data-line or class
    const lineEl = document.querySelector(`[data-line="${lineNumber}"]`)
      || document.querySelector(`.code-line:nth-child(${lineNumber})`)
      || document.getElementById(`line-${lineNumber}`);

    if (lineEl) {
      const rect = lineEl.getBoundingClientRect();
      return {
        rect,
        centerX: rect.left + rect.width * 0.4,
        centerY: rect.top + rect.height * 0.5,
        element: lineEl
      };
    }

    // Fallback: estimate from editor container
    const editorRect = this.editorContainer ? this.editorContainer.getBoundingClientRect() : { left: 300, top: 100, width: 800, height: 600 };
    const lineHeight = 22;
    const estimatedY = editorRect.top + 40 + (lineNumber * lineHeight);
    return {
      rect: { left: editorRect.left + 50, top: estimatedY, width: 600, height: lineHeight },
      centerX: editorRect.left + 250,
      centerY: estimatedY + lineHeight * 0.5,
      element: null
    };
  }

  // Calculate non-occluding parking coordinate for Phi
  calculateParkingPosition(targetRect) {
    const mascotWidth = 240;
    const mascotHeight = 240;
    const margin = 25;

    // Prefer parking on the right side of the code if there's enough screen real estate
    const spaceOnRight = window.innerWidth - (targetRect.left + targetRect.width);
    const spaceOnLeft = targetRect.left;

    let parkX, parkY;

    if (spaceOnRight >= mascotWidth + margin) {
      // Park to the right of code block
      parkX = targetRect.left + targetRect.width + margin;
      parkY = Math.max(80, targetRect.top - mascotHeight * 0.3);
    } else if (spaceOnLeft >= mascotWidth + margin) {
      // Park to the left (in gutter / margin area)
      parkX = Math.max(margin, targetRect.left - mascotWidth - margin);
      parkY = Math.max(80, targetRect.top - mascotHeight * 0.3);
    } else {
      // Park floating slightly above and to the right
      parkX = Math.min(window.innerWidth - mascotWidth - margin, targetRect.left + 150);
      parkY = Math.max(60, targetRect.top - mascotHeight - 10);
    }

    // Keep within viewport bounds
    parkX = Math.max(10, Math.min(window.innerWidth - mascotWidth - 10, parkX));
    parkY = Math.max(10, Math.min(window.innerHeight - mascotHeight - 10, parkY));

    return { parkX, parkY };
  }

  // Focus on a specific line with animation, laser, and optional speech
  async focusLine(lineNumber, options = {}) {
    const opts = Object.assign({
      spokenText: '',
      speechStatus: `Line ${lineNumber} · Focus`,
      emotion: 'focused',
      laser: true,
      scrollIntoView: true,
      dwellMs: 1200
    }, options);

    const target = this.resolveLineCoordinates(lineNumber);

    // Auto-scroll target into view if needed
    if (opts.scrollIntoView && target.element) {
      target.element.scrollIntoView({ behavior: 'smooth', block: 'center' });
      // Re-read coordinates after scroll animation starts
      await new Promise(r => setTimeout(r, 150));
    }

    const { parkX, parkY } = this.calculateParkingPosition(target.rect);

    // Set Phi's emotion
    this.rig.setEmotion(opts.emotion);

    // Fly smoothly to the parking spot
    this.rig.flyTo(parkX, parkY);

    // Point gaze directly at target center
    this.rig.gazeAt(target.centerX, target.centerY);

    // Highlight the code line with cybernetic spotlight brackets
    this.applySpotlight(target.rect);

    // Fire laser if enabled
    if (opts.laser) {
      this.rig.fireLaser(target.centerX, target.centerY, true);
    }

    // Speak explanation with synchronized mouth visemes
    if (opts.spokenText) {
      this.rig.setSpeechText(opts.spokenText, opts.speechStatus);
      await this.viseme.speak(opts.spokenText, {
        onWord: (word) => {
          // Subtle pulse on each word spoken
          this.pulseSpotlight();
        }
      });
    } else {
      await new Promise(r => setTimeout(r, opts.dwellMs));
    }
  }

  applySpotlight(rect) {
    this.spotlightOverlay.style.display = 'block';
    this.spotlightOverlay.style.left = `${rect.left - 20}px`;
    this.spotlightOverlay.style.top = `${rect.top - 2}px`;
    this.spotlightOverlay.style.width = `${rect.width + 40}px`;
    this.spotlightOverlay.style.height = `${rect.height + 4}px`;
    this.spotlightOverlay.classList.remove('pulse');
    void this.spotlightOverlay.offsetWidth; // trigger reflow
    this.spotlightOverlay.classList.add('pulse');
  }

  pulseSpotlight() {
    this.spotlightOverlay.style.boxShadow = '0 0 25px rgba(251, 191, 36, 0.7), inset 0 0 15px rgba(251, 191, 36, 0.3)';
    setTimeout(() => {
      this.spotlightOverlay.style.boxShadow = '0 0 15px rgba(251, 191, 36, 0.4), inset 0 0 8px rgba(251, 191, 36, 0.15)';
    }, 120);
  }

  clearFocus() {
    this.spotlightOverlay.style.display = 'none';
    this.rig.stopLaser();
    this.rig.setEmotion('curious');
  }
}
