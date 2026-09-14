// Local semantic IDE events only. Never send source, paths, prompts or keys.
export class PhiIdeFrictionClient {
    constructor({ getToken = () => null, fetch: fetcher = globalThis.fetch.bind(globalThis),
        now = () => performance.now(), clock = Date.now,
        visible = () => document.visibilityState === 'visible' && document.hasFocus(),
        onStatus = () => {}, enabled = true, timers = globalThis } = {}) {
        Object.assign(this, { getToken, fetcher, now, clock, visible, onStatus, timers, enabled });
        this.queue = []; this.review = null; this.activeMs = 0;
        this.lastActivity = -Infinity; this.lastTick = now(); this.destroyed = false;
        this.sequence = 0; this.epoch = 0; this.instance = crypto.randomUUID();
        this.timer = timers.setInterval(() => this.tick(), 1000);
    }

    setEnabled(enabled) {
        this.epoch += 1;
        this.enabled = !!enabled; this.queue = []; this.activeMs = 0;
        this.lastActivity = -Infinity; this.lastTick = this.now();
        if (this.review) this.review.activeMs = 0;
        if (!this.enabled) this.controller?.abort();
    }

    activity() {
        this.tick(false);
        if (this.review) this.review.lastActivity = -Infinity;
        if (this.enabled && this.visible()) this.lastActivity = this.now();
    }
    reviewActivity() {
        this.activity();
        if (this.enabled && this.visible() && this.review) this.review.lastActivity = this.now();
    }

    // Native semantic undo events are observable; generation attribution is not
    // inferred from timing when the editor did not apply that generated patch.
    undo() { this.activity(); this.report('editor_undo', { count: 1 }); }

    beginReview({ generationId, addedLines, digest, isVisible }) {
        this.endReview('closed');
        if (!/^[A-Za-z0-9_-]{1,96}$/.test(generationId || '') ||
            !Number.isSafeInteger(addedLines) || addedLines < 0 || addedLines > 1000000 ||
            !/^[a-f0-9]{64}$/.test(digest || '')) return;
        this.review = { generationId, addedLines, digest, isVisible, activeMs: 0,
            lastActivity: -Infinity, decision: null };
    }

    endReview(decision = 'closed', generationId = this.review?.generationId) {
        const review = this.review;
        if (!review || generationId !== review.generationId ||
            !['closed', 'accepted', 'rejected'].includes(decision)) return false;
        this.tick(false);
        // Dismissing/reopening a preview cannot create repeated rejections.
        if (review.decision === decision) return false;
        this.report('review_closed', {
            decision, added_lines: review.addedLines,
            active_review_ms: Math.min(86400000, Math.floor(review.activeMs)), diff_digest: review.digest,
        }, review.generationId);
        review.decision = decision;
        if (decision !== 'rejected') this.review = null;
        return true;
    }

    tick(flush = true) {
        if (this.destroyed) return;
        const now = this.now(), elapsed = now - this.lastTick;
        this.lastTick = now;
        // Suspended tabs/processes do not accumulate activity on resume.
        const active = this.enabled && this.visible() && now - this.lastActivity <= 30000;
        if (active && elapsed > 0 && elapsed <= 2000) {
            this.activeMs += elapsed;
            if (this.review && !this.review.decision && now - this.review.lastActivity <= 30000 &&
                this.review.isVisible()) this.review.activeMs += elapsed;
        }
        if (this.activeMs >= 15000) {
            this.report('activity', { active_ms: Math.min(60000, Math.floor(this.activeMs)),
                local_hour: new Date(this.clock()).getHours() });
            this.activeMs = 0;
        }
        if (flush) void this.flush();
    }

    report(kind, data, generationId) {
        if (!this.enabled || this.destroyed || !this.getToken()) return;
        const event = { id: `${this.instance}-${++this.sequence}`, kind, data };
        if (generationId) event.generation_id = generationId;
        this.queue.push(event);
        if (this.queue.length > 32) this.queue.shift();
    }

    async flush() {
        if (this.busy || this.destroyed || !this.enabled || !this.queue.length) return;
        const token = this.getToken();
        if (!token) { this.queue = []; return; }
        const events = this.queue.splice(0, 32);
        const epoch = this.epoch;
        this.busy = true;
        const controller = new AbortController(); this.controller = controller;
        const deadline = this.timers.setTimeout(() => controller.abort(), 5000);
        try {
            const response = await this.fetcher('/api/friction/events', { method: 'POST',
                headers: { 'content-type': 'application/json', 'x-selfware-session': token },
                credentials: 'same-origin', redirect: 'error', signal: controller.signal,
                body: JSON.stringify({ events }) });
            if (!response.ok || response.redirected || !response.headers.get('content-type')?.includes('application/json')) throw Error('invalid_ack');
            const reader = response.body.getReader(); let length = 0; const chunks = [];
            const abortRead = () => { void reader.cancel().catch(() => {}); };
            controller.signal.addEventListener('abort', abortRead, { once: true });
            try {
                for (;;) {
                    if (controller.signal.aborted) throw Error('aborted');
                    const part = await reader.read();
                    if (controller.signal.aborted) throw Error('aborted');
                    if (part.done) break;
                    length += part.value.length;
                    if (length > 1024) throw Error('invalid_ack');
                    chunks.push(part.value);
                }
            } catch (error) { void reader.cancel().catch(() => {}); throw error; }
            finally { controller.signal.removeEventListener('abort', abortRead); reader.releaseLock(); }
            const bytes = new Uint8Array(length); let offset = 0;
            for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length; }
            const result = JSON.parse(new TextDecoder().decode(bytes));
            if (!Number.isSafeInteger(result.accepted) || result.accepted < 0 || result.accepted > events.length ||
                !Number.isSafeInteger(result.cursor) || result.cursor < 0) throw Error('invalid_ack');
            if (!this.destroyed && this.enabled && epoch === this.epoch) this.onStatus('connected');
        } catch (_) { if (!this.destroyed && this.enabled && epoch === this.epoch) this.onStatus('unavailable'); }
        finally { this.timers.clearTimeout(deadline); this.busy = false; this.controller = null; }
        // Best effort, no persistent queue or late replay of stale observations.
    }

    destroy() {
        this.epoch += 1;
        this.destroyed = true; this.queue = []; this.review = null;
        this.timers.clearInterval(this.timer); this.controller?.abort();
    }
}
