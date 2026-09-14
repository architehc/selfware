/** Measured source ranges, cancellable focus, and viewport-relative targeting. */
export class PhiFocusCoordinator {
  constructor(rig, visemeEngine, editorContainer) {
    this.rig = rig;
    this.viseme = visemeEngine;
    this.editorContainer = editorContainer;
    this.active = null;
    this.parkingPosition = null;
    this.sequence = 0;
    this.refreshFrame = null;
    this.extraOverlays = [];
    this.spotlightOverlay = this.makeOverlay();
    this.refreshHandler = () => {
      if (this.refreshFrame !== null) return;
      this.refreshFrame = requestAnimationFrame(() => { this.refreshFrame = null; this.refresh(); });
    };
    document.addEventListener('scroll', this.refreshHandler, true);
    window.addEventListener('resize', this.refreshHandler);
    if (typeof ResizeObserver !== 'undefined' && editorContainer) {
      this.resizeObserver = new ResizeObserver(this.refreshHandler);
      this.resizeObserver.observe(editorContainer);
    }
    if (typeof MutationObserver !== 'undefined' && editorContainer) {
      this.mutationObserver = new MutationObserver(this.refreshHandler);
      this.mutationObserver.observe(editorContainer, { subtree: true, childList: true, characterData: true });
    }
  }
  makeOverlay() {
    const overlay = document.createElement('div');
    overlay.className = 'phi-code-spotlight';
    Object.assign(overlay.style, { position: 'fixed', pointerEvents: 'none', display: 'none', zIndex: '50', transition: 'box-shadow .12s ease' });
    overlay.setAttribute('aria-hidden', 'true');
    document.body.appendChild(overlay);
    return overlay;
  }
  lineElement(line) {
    if (!Number.isInteger(line) || line < 1) return null;
    const root = this.editorContainer || document;
    return root.querySelector(`[data-line="${line}"]`) || root.querySelector(`.code-line:nth-child(${line})`);
  }
  textElement(line) {
    const element = this.lineElement(line);
    return element?.querySelector('.line-code') || element;
  }
  textPosition(element, offset) {
    const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
    let node, remaining = offset, last = null;
    while ((node = walker.nextNode())) {
      last = node;
      if (remaining <= node.textContent.length) return [node, remaining];
      remaining -= node.textContent.length;
    }
    if (remaining === 0 && last) return [last, last.textContent.length];
    if (offset === 0 && !last) return [element, 0];
    throw new RangeError('Source offset outside displayed text');
  }
  resolveRange(descriptor) {
    const line = descriptor.line, endLine = descriptor.endLine ?? line;
    if (!Number.isInteger(line) || !Number.isInteger(endLine) || endLine < line) return null;
    const first = this.textElement(line), last = this.textElement(endLine);
    if (!first || !last) return null;
    const start = descriptor.start ?? 0, end = descriptor.end ?? last.textContent.length;
    if (!Number.isInteger(start) || !Number.isInteger(end) || start < 0 || end < 0 || (line === endLine && end < start)) return null;
    try {
      const range = document.createRange();
      range.setStart(...this.textPosition(first, start));
      range.setEnd(...this.textPosition(last, end));
      const elements = [];
      for (let n = line; n <= endLine; n++) {
        const element = this.textElement(n);
        if (!element) return null;
        elements.push({ element, text: element.textContent });
      }
      return { descriptor: { line, endLine, start, end }, range, elements, element: this.lineElement(line),
        text: elements.map((entry, index) => entry.text.slice(index === 0 ? start : 0, index === elements.length - 1 ? end : undefined)).join('\n') };
    } catch (_) { return null; }
  }
  measure(target) {
    if (target.elements.some(e => !e.element.isConnected || e.element.textContent !== e.text)) return null;
    // Measure each source line independently so line numbers/gutters between
    // multiline endpoints never become part of the highlighted source range.
    let rects = [];
    for (let i = 0; i < target.elements.length; i++) {
      const entry = target.elements[i], range = document.createRange();
      range.setStart(...this.textPosition(entry.element, i === 0 ? target.descriptor.start : 0));
      range.setEnd(...this.textPosition(entry.element, i === target.elements.length - 1 ? target.descriptor.end : entry.text.length));
      rects.push(...[...range.getClientRects()].filter(r => r.width > 0 && r.height > 0));
    }
    if (!rects.length) {
      const rect = target.element.getBoundingClientRect();
      if (rect.height > 0) rects.push(rect);
    }
    if (!rects.length) return { visible: false, rects: [], element: target.element };
    // Fixed overlays must obey the source's actual scrollport clipping. A range
    // scrolled beneath a toolbar is still connected but is no longer visible.
    let clip = { left: 0, top: 0, right: window.innerWidth, bottom: window.innerHeight };
    for (let element = target.element.parentElement; element; element = element.parentElement) {
      const style = getComputedStyle(element), bounds = element.getBoundingClientRect();
      if (/^(auto|scroll|hidden|clip)$/.test(style.overflowX)) {
        clip.left = Math.max(clip.left, bounds.left + element.clientLeft);
        clip.right = Math.min(clip.right, bounds.left + element.clientLeft + element.clientWidth);
      }
      if (/^(auto|scroll|hidden|clip)$/.test(style.overflowY)) {
        clip.top = Math.max(clip.top, bounds.top + element.clientTop);
        clip.bottom = Math.min(clip.bottom, bounds.top + element.clientTop + element.clientHeight);
      }
    }
    rects = rects.map(rect => {
      const left = Math.max(rect.left, clip.left), right = Math.min(rect.right, clip.right);
      const top = Math.max(rect.top, clip.top), bottom = Math.min(rect.bottom, clip.bottom);
      return { left, right, top, bottom, width: right - left, height: bottom - top };
    }).filter(rect => rect.width > 0 && rect.height > 0);
    rects = [...new Map(rects.map(rect => [[rect.left, rect.top, rect.right, rect.bottom].join(','), rect])).values()];
    if (!rects.length) return { visible: false, rects: [], element: target.element };
    const left = Math.min(...rects.map(r => r.left)), top = Math.min(...rects.map(r => r.top));
    const right = Math.max(...rects.map(r => r.right)), bottom = Math.max(...rects.map(r => r.bottom));
    return { visible: true, rects, rect: { left, top, right, bottom, width: right - left, height: bottom - top }, element: target.element,
      centerX: (rects[0].left + rects[0].right) / 2, centerY: (rects[0].top + rects[0].bottom) / 2 };
  }
  resolveLineCoordinates(lineNumber) {
    const target = this.resolveRange({ line: lineNumber });
    return target ? this.measure(target) : null;
  }
  calculateParkingPosition(rect) {
    const size = this.rig.getLayoutSize?.() || { width: this.rig.wrapper?.getBoundingClientRect().width || 240, height: this.rig.wrapper?.getBoundingClientRect().height || 240 };
    const left = size.leftInset || 0, right = size.rightInset || 0, top = size.topInset || 0, bottom = size.bottomInset || 0;
    const bounds = this.rig.getParkingBounds?.() || { minX: 10 + left, maxX: window.innerWidth - size.width - right - 10, minY: 10 + top, maxY: window.innerHeight - size.height - bottom - 10 };
    const clamp = (v, lo, hi) => Math.max(lo, Math.min(Math.max(lo, hi), v));
    const union = p => ({ left: p.parkX - left, top: p.parkY - top,
      right: p.parkX + size.width + right, bottom: p.parkY + size.height + bottom });
    const overlap = (a, b) => Math.max(0, Math.min(a.right, b.right) - Math.max(a.left, b.left))
      * Math.max(0, Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top));
    const visible = r => r.width > 0 && r.height > 0 && r.right > 0 && r.bottom > 0 && r.left < window.innerWidth && r.top < window.innerHeight;
    const controls = [...document.querySelectorAll('button,input,textarea,select,[contenteditable="true"],[role="button"],.narration-panel')]
      .filter(element => !this.rig.wrapper?.contains(element))
      .map(element => element.getBoundingClientRect()).filter(visible);
    const perch = document.getElementById('phi-perch')?.getBoundingClientRect();
    const positions = [
      [rect.right + 20 + left, rect.top + top],
      [rect.left - size.width - right - 20, rect.top + top],
      [rect.left, rect.top - size.height - bottom - 20],
      [rect.left, rect.bottom + top + 20],
      [bounds.minX, bounds.minY], [bounds.maxX, bounds.minY],
      [bounds.minX, bounds.maxY], [bounds.maxX, bounds.maxY]
    ];
    if (perch && visible(perch)) positions.unshift([perch.left + (perch.width - size.width - left - right) / 2 + left, perch.top + top + 12]);
    for (const control of controls) positions.push(
      [control.right + 12 + left, control.top + top],
      [control.left - size.width - right - 12, control.top + top],
      [control.left + left, control.top - size.height - bottom - 12],
      [control.left + left, control.bottom + top + 12]
    );
    const candidates = positions.map(([x, y]) => ({ parkX: clamp(x, bounds.minX, bounds.maxX), parkY: clamp(y, bounds.minY, bounds.maxY) }));
    const score = p => {
      const body = union(p);
      // Source, captions and interactive controls are exclusions for the entire rig + HUD.
      // If the viewport cannot fit a clear placement, choose the least overlap;
      // viewport containment remains the rig's responsibility at every frame.
      return overlap(rect, body) + controls.reduce((sum, control) => sum + overlap(control, body), 0);
    };
    if (this.parkingPosition) {
      // Animated ears/tails and caption reflow can make another clear slot rank
      // first on a later word. Keep this slot while it remains clear, allowing
      // only the viewport clamp to move it after a resize or bounds change.
      const previous = { parkX: clamp(this.parkingPosition.parkX, bounds.minX, bounds.maxX),
        parkY: clamp(this.parkingPosition.parkY, bounds.minY, bounds.maxY) };
      if (score(previous) === 0) { this.parkingPosition = previous; return previous; }
    }
    candidates.sort((a, b) => score(a) - score(b));
    this.parkingPosition = candidates[0];
    return this.parkingPosition;
  }
  delay(ms, request) {
    return new Promise(resolve => {
      if (request.abort.signal.aborted) { resolve(false); return; }
      const done = value => { clearTimeout(timer); request.abort.signal.removeEventListener('abort', aborted); resolve(value); };
      const aborted = () => done(false), timer = setTimeout(() => done(true), ms);
      request.abort.signal.addEventListener('abort', aborted, { once: true });
    });
  }
  async waitForScroll(request) {
    let previous = null, stable = 0;
    for (let elapsed = 0; elapsed < 900; elapsed += 25) {
      if (!await this.delay(25, request)) return false;
      const measured = this.measure(request.target);
      if (!measured) return false;
      if (!measured.visible) continue;
      const position = [measured.rect.left, measured.rect.top].join(',');
      stable = position === previous ? stable + 1 : 0; previous = position;
      if (elapsed >= 125 && stable >= 3) return true;
    }
    return !request.abort.signal.aborted && this.measure(request.target)?.visible === true;
  }
  focusLine(lineNumber, options = {}) { return this.focusRange({ line: lineNumber, start: options.start, end: options.end, endLine: options.endLine }, options); }
  async focusRange(descriptor, options = {}) {
    this.cancel('superseded');
    const target = this.resolveRange(descriptor);
    if (!target) return { status: 'error', reason: 'source_range_unavailable' };
    const opts = { spokenText: '', speechStatus: `Line ${descriptor.line} · Focus`, emotion: 'focused', laser: true, scrollIntoView: true, dwellMs: 1200, ...options };
    const request = { id: ++this.sequence, target, options: opts, abort: new AbortController(), wordTarget: null };
    this.active = request;
    if (opts.scrollIntoView) {
      target.element.scrollIntoView({ behavior: window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ? 'instant' : 'smooth', block: 'center', inline: 'nearest' });
      if (!await this.waitForScroll(request)) {
        if (this.active === request) {
          this.cancel('source_not_visible');
          return { status: 'error', reason: 'source_range_not_visible' };
        }
        return { status: 'cancelled', reason: 'focus_superseded' };
      }
    }
    if (this.active !== request || request.abort.signal.aborted) return { status: 'cancelled', reason: 'focus_superseded' };
    this.rig.setEmotion(opts.emotion);
    if (opts.spokenText) this.rig.setSpeechText(opts.spokenText, opts.speechStatus);
    if (!this.refresh()) return { status: 'error', reason: 'source_range_unavailable' };
    if (opts.spokenText) {
      const speech = await this.viseme.speak(opts.spokenText, {
        ...(opts.speechOptions || {}), ...(opts.speechRate !== undefined ? { speechRate: opts.speechRate } : {}),
        onWord: (word, charIndex, detail) => {
          if (this.active !== request || request.abort.signal.aborted) return;
          // Explanation text is not source text. Token tracking is enabled only
          // when the utterance exactly matches the selected single-line source.
          if (opts.spokenText === target.text && target.descriptor.line === target.descriptor.endLine && detail) {
            request.wordTarget = this.resolveRange({ line: target.descriptor.line, start: target.descriptor.start + detail.charStart, end: target.descriptor.start + detail.charEnd });
            this.refresh();
          }
          this.pulseSpotlight(); (opts.onWord || opts.speechOptions?.onWord)?.(word, charIndex, detail);
        }
      });
      if (this.active !== request || request.abort.signal.aborted) return { status: 'cancelled', reason: 'focus_superseded' };
      return { status: speech?.status || 'completed', speech, range: target.descriptor };
    }
    const completed = await this.delay(opts.dwellMs, request);
    return { status: completed && this.active === request ? 'completed' : 'cancelled', range: target.descriptor };
  }
  refresh() {
    const request = this.active;
    if (!request || request.abort.signal.aborted) return false;
    const anchor = this.measure(request.target);
    if (!anchor) { this.cancel('source_changed'); return false; }
    if (!anchor.visible) {
      this.applySpotlight([]); this.rig.stopLaser(); this.rig.setAvoidRect?.(null);
      return true;
    }
    const measured = request.wordTarget ? this.measure(request.wordTarget) || anchor : anchor;
    // Word tracking changes the spotlight and gaze, but the whole selected
    // source remains readable while its narration is in progress.
    this.rig.setAvoidRect?.(anchor.rect);
    const { parkX, parkY } = this.calculateParkingPosition(anchor.rect);
    this.rig.flyTo(parkX, parkY);
    if (!measured.visible) { this.applySpotlight([]); this.rig.stopLaser(); return true; }
    this.rig.gazeAt(measured.centerX, measured.centerY);
    this.applySpotlight(measured.rects);
    if (request.options.laser) this.rig.fireLaser(measured.centerX, measured.centerY, true);
    else this.rig.stopLaser();
    return true;
  }
  applySpotlight(input) {
    const rects = Array.isArray(input) ? input : [input];
    while (this.extraOverlays.length < rects.length - 1) this.extraOverlays.push(this.makeOverlay());
    const overlays = [this.spotlightOverlay, ...this.extraOverlays];
    overlays.forEach((overlay, index) => {
      const rect = rects[index];
      if (!rect) { overlay.style.display = 'none'; return; }
      Object.assign(overlay.style, { display: 'block', left: `${rect.left}px`, top: `${rect.top}px`, width: `${rect.width}px`, height: `${rect.height}px` });
    });
  }
  pulseSpotlight() {
    this.spotlightOverlay.classList.remove('pulse');
    void this.spotlightOverlay.offsetWidth;
    this.spotlightOverlay.classList.add('pulse');
  }
  cancel(reason = 'cancelled') {
    if (this.active) this.active.abort.abort();
    this.active = null;
    this.parkingPosition = null;
    this.viseme.stop(reason);
    for (const overlay of [this.spotlightOverlay, ...this.extraOverlays]) overlay.style.display = 'none';
    this.rig.stopLaser(); this.rig.setAvoidRect?.(null);
    return this;
  }
  clearFocus() { this.cancel('focus_cleared'); this.rig.setEmotion('curious'); }
  destroy() {
    this.cancel('destroyed');
    if (this.refreshFrame !== null) cancelAnimationFrame(this.refreshFrame);
    document.removeEventListener('scroll', this.refreshHandler, true); window.removeEventListener('resize', this.refreshHandler);
    this.resizeObserver?.disconnect(); this.mutationObserver?.disconnect();
    for (const overlay of [this.spotlightOverlay, ...this.extraOverlays]) overlay.remove();
  }
}
