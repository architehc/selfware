// Same-origin local speech transport. This module neither plays audio nor executes model output.
const JOB_ID = /^[0-9a-f]{32}$/;
const SHA256 = /^[0-9a-f]{64}$/;
const STATES = new Set(['queued', 'running', 'done', 'failed', 'cancelled']);
const AUDIO_LIMIT = 32 * 1024 * 1024;
const JSON_LIMIT = 1024 * 1024;

export class SpeechClientError extends Error {
  constructor(code, message, status = 0, detail = null) {
    super(message); this.name = 'SpeechClientError'; this.code = code;
    this.status = status; this.detail = detail;
  }
}

const fail = (code, message) => { throw new SpeechClientError(code, message); };
function checkAbort(signal) {
  if (signal?.aborted) throw signal.reason instanceof SpeechClientError ? signal.reason :
    new SpeechClientError('cancelled', 'Speech was cancelled.');
}
function validateId(id) {
  if (typeof id !== 'string' || !JOB_ID.test(id)) fail('invalid_job', 'Invalid speech job identifier.');
  return id;
}
function notify(callback, value) { try { callback?.(value); } catch (_) { /* Observers do not own transport. */ } }
function delay(ms, signal) {
  return new Promise((resolve, reject) => {
    const abort = () => { clearTimeout(timer); signal?.removeEventListener('abort', abort); try { checkAbort(signal); } catch (error) { reject(error); } };
    const timer = setTimeout(() => { signal?.removeEventListener('abort', abort); resolve(); }, ms);
    if (signal?.aborted) abort(); else signal?.addEventListener('abort', abort, { once: true });
  });
}

export class PhiSpeechClient {
  constructor({ getToken, fetch: fetcher = globalThis.fetch?.bind(globalThis), pollIntervalMs = 300 } = {}) {
    this.getToken = getToken; this.fetch = fetcher;
    this.pollIntervalMs = Math.max(10, Math.min(2000, pollIntervalMs));
  }

  async request(path, { body, signal, timeoutMs = 15000, binary = false, maxBytes = JSON_LIMIT } = {}) {
    checkAbort(signal);
    const token = this.getToken?.();
    if (typeof token !== 'string' || !token || /[\r\n]/.test(token)) fail('session_required', 'Connect a Selfware workspace before using local speech.');
    if (!/^\/api\/speech\/(?:capabilities|jobs(?:\/[0-9a-f]{32}(?:\/(?:audio|cancel))?)?)$/.test(path)) fail('invalid_path', 'Speech requests must stay on the same-origin speech API.');
    const controller = new AbortController();
    const abort = () => controller.abort(signal.reason);
    signal?.addEventListener('abort', abort, { once: true });
    const timer = setTimeout(() => controller.abort(new SpeechClientError('timeout', 'Local speech request timed out.')), timeoutMs);
    try {
      const response = await this.fetch(path, {
        method: body === undefined ? 'GET' : 'POST', redirect: 'error', credentials: 'same-origin', cache: 'no-store',
        signal: controller.signal,
        headers: { Accept: binary ? 'audio/wav' : 'application/json', 'x-selfware-session': token,
          ...(body === undefined ? {} : { 'Content-Type': 'application/json' }) },
        ...(body === undefined ? {} : { body: JSON.stringify(body) }),
      });
      checkAbort(controller.signal);
      if (response.redirected) fail('redirect_rejected', 'Local speech redirects are not accepted.');
      const type = (response.headers.get('content-type') || '').split(';')[0].trim().toLowerCase();
      const isJSON = type === 'application/json';
      const limit = response.ok && binary ? maxBytes : JSON_LIMIT;
      const declared = response.headers.get('content-length');
      if (declared !== null && (!/^\d+$/.test(declared) || Number(declared) > limit)) fail('response_too_large', 'Local speech response exceeds its size limit.');
      if (response.ok && (binary ? !['audio/wav', 'audio/x-wav'].includes(type) : !isJSON)) fail('invalid_response', 'Local speech returned an unexpected content type.');
      const reader = response.body?.getReader();
      if (!reader) fail('invalid_response', 'Local speech returned no response body.');
      const abortRead = () => { void reader.cancel().catch(() => {}); };
      controller.signal.addEventListener('abort', abortRead, { once: true });
      const chunks = []; let size = 0;
      try {
        while (true) {
          checkAbort(controller.signal);
          const { value, done } = await reader.read();
          checkAbort(controller.signal);
          if (done) break;
          size += value.byteLength;
          if (size > limit) fail('response_too_large', 'Local speech response exceeds its size limit.');
          chunks.push(value);
        }
      } catch (error) { await reader.cancel().catch(() => {}); throw error; }
      finally { controller.signal.removeEventListener('abort', abortRead); reader.releaseLock(); }
      const bytes = new Uint8Array(size); let offset = 0;
      for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
      if (declared !== null && size !== Number(declared)) fail('truncated_response', 'Local speech response was truncated.');
      let payload;
      if (isJSON) { try { payload = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes)); } catch (_) { fail('invalid_response', 'Local speech returned invalid JSON.'); } }
      if (!response.ok) {
        const detail = payload?.error || payload;
        throw new SpeechClientError(typeof detail?.code === 'string' ? detail.code : 'http_error',
          typeof detail?.message === 'string' ? detail.message.slice(0, 1000) : `Local speech request failed (${response.status}).`, response.status, payload);
      }
      return binary ? bytes : payload;
    } catch (error) {
      if (controller.signal.aborted) checkAbort(controller.signal);
      if (error instanceof SpeechClientError) throw error;
      throw new SpeechClientError('network_error', 'The local speech service could not be reached.');
    } finally { clearTimeout(timer); signal?.removeEventListener('abort', abort); }
  }

  async capabilities(options = {}) {
    const value = await this.request('/api/speech/capabilities', options);
    if (!value || typeof value.configured !== 'boolean' || typeof value.status !== 'string') fail('invalid_capabilities', 'Invalid local speech capabilities.');
    if (value.configured) {
      if (!['loading', 'ready', 'failed'].includes(value.status) || value.provider !== 'vibevoice_onnx' ||
          !Array.isArray(value.voices) || value.voices.length > 100 ||
          !value.voices.every(voice => typeof voice.id === 'string' && voice.id.length > 0 && voice.id.length <= 100 && typeof voice.name === 'string') ||
          new Set(value.voices.map(voice => voice.id)).size !== value.voices.length ||
          !Number.isInteger(value.max_text_chars) || value.max_text_chars < 1 || value.max_text_chars > 5000) {
        fail('invalid_capabilities', 'Invalid local speech capabilities.');
      }
    }
    return value;
  }

  validateJob(job, id) {
    if (!job || !JOB_ID.test(job.id) || (id && job.id !== id) || !STATES.has(job.status) ||
        typeof job.text !== 'string' || typeof job.voice !== 'string') fail('invalid_job', 'Invalid local speech job response.');
    if (job.status === 'done') {
      const a = job.audio;
      if (!a || a.url !== `/api/speech/jobs/${job.id}/audio` || !Number.isInteger(a.bytes) || a.bytes < 44 || a.bytes > AUDIO_LIMIT ||
          !SHA256.test(a.sha256) || !Number.isInteger(a.sample_rate) || a.sample_rate <= 0 ||
          !Number.isFinite(a.duration) || a.duration <= 0 || a.channels !== 1) fail('invalid_audio', 'Invalid local speech audio metadata.');
      if (job.alignment?.status !== 'unavailable') fail('unsupported_alignment', 'Local speech alignment format is not supported.');
    }
    return job;
  }

  async create(text, voice, options = {}) {
    if (typeof text !== 'string' || !text.trim() || [...text].length > 5000) fail('invalid_text', 'Local speech accepts 1–5000 characters.');
    if (typeof voice !== 'string' || !voice || voice.length > 100) fail('invalid_voice', 'Choose a supported local voice.');
    const requestId = options.requestId;
    if (requestId !== undefined) validateId(requestId);
    return this.validateJob(await this.request('/api/speech/jobs', { ...options, body: { text, voice, ...(requestId ? { request_id: requestId } : {}) } }), requestId);
  }
  async status(id, options = {}) {
    return this.validateJob(await this.request(`/api/speech/jobs/${validateId(id)}`, options), id);
  }
  async cancel(id, options = {}) {
    return this.validateJob(await this.request(`/api/speech/jobs/${validateId(id)}/cancel`, { ...options, body: {}, timeoutMs: 3000 }), id);
  }
  async audio(job, options = {}) {
    this.validateJob(job);
    if (job.status !== 'done') fail('audio_not_ready', 'Local speech audio is not ready.');
    const bytes = await this.request(job.audio.url, { ...options, binary: true, maxBytes: job.audio.bytes });
    if (bytes.length !== job.audio.bytes) fail('truncated_audio', 'Local speech audio length does not match its receipt.');
    const tag = (start, end) => String.fromCharCode(...bytes.slice(start, end));
    if (tag(0, 4) !== 'RIFF' || tag(8, 12) !== 'WAVE' || new DataView(bytes.buffer).getUint32(4, true) + 8 !== bytes.length) fail('invalid_audio', 'Local speech audio is not a complete WAV file.');
    const view = new DataView(bytes.buffer); let format = null, dataBytes = 0;
    for (let offset = 12; offset < bytes.length;) {
      if (offset + 8 > bytes.length) fail('invalid_audio', 'Local speech WAV has a truncated chunk.');
      const length = view.getUint32(offset + 4, true), start = offset + 8;
      if (start + length > bytes.length) fail('invalid_audio', 'Local speech WAV has a truncated chunk.');
      if (tag(offset, offset + 4) === 'fmt ') {
        if (length < 16) fail('invalid_audio', 'Local speech WAV has an invalid format.');
        format = { encoding: view.getUint16(start, true), channels: view.getUint16(start + 2, true),
          rate: view.getUint32(start + 4, true), align: view.getUint16(start + 12, true), bits: view.getUint16(start + 14, true) };
      }
      if (tag(offset, offset + 4) === 'data') dataBytes += length;
      offset = start + length + (length % 2);
    }
    if (!format || ![1, 3].includes(format.encoding) || format.channels !== 1 || format.rate !== job.audio.sample_rate ||
        ![8, 16, 24, 32, 64].includes(format.bits) || format.align !== format.bits / 8 ||
        !dataBytes || dataBytes % format.align || Math.abs(dataBytes / format.align / format.rate - job.audio.duration) > 1 / format.rate) {
      fail('invalid_audio', 'Local speech WAV measurements do not match its receipt.');
    }
    if (!globalThis.crypto?.subtle) fail('integrity_unavailable', 'Audio integrity verification requires a secure browser context.');
    const hash = Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)), byte => byte.toString(16).padStart(2, '0')).join('');
    checkAbort(options.signal);
    if (hash !== job.audio.sha256) fail('audio_integrity', 'Local speech audio does not match its receipt.');
    return new Blob([bytes], { type: 'audio/wav' });
  }

  // Returns owned audio. The playback session MUST call dispose() on every terminal path.
  async synthesize(text, voice, { signal, onStatus, timeoutMs = 180000 } = {}) {
    if (!Number.isFinite(timeoutMs) || timeoutMs <= 0 || timeoutMs > 600000) fail('invalid_timeout', 'Invalid local speech deadline.');
    checkAbort(signal);
    if (!globalThis.crypto?.randomUUID) fail('secure_context_required', 'Local speech requires a secure browser context.');
    const requestId = crypto.randomUUID().replaceAll('-', '');
    const controller = new AbortController();
    const abort = () => controller.abort(signal.reason);
    signal?.addEventListener('abort', abort, { once: true });
    const timer = setTimeout(() => controller.abort(new SpeechClientError('timeout', 'Local speech generation timed out.')), timeoutMs);
    let job;
    try {
      notify(onStatus, { status: 'submitting', provider: 'vibevoice_onnx' });
      job = await this.create(text, voice, { signal: controller.signal, requestId });
      while (true) {
        checkAbort(controller.signal);
        if (job.text !== text || job.voice !== voice) fail('job_mismatch', 'Local speech returned a different narration or voice.');
        notify(onStatus, job);
        if (job.status === 'done') break;
        if (job.status === 'failed' || job.status === 'cancelled') throw new SpeechClientError(job.error?.code || job.status, job.error?.message || `Local speech ${job.status}.`, 0, job);
        await delay(this.pollIntervalMs, controller.signal);
        job = await this.status(job.id, { signal: controller.signal });
      }
      notify(onStatus, { ...job, status: 'downloading' });
      const blob = await this.audio(job, { signal: controller.signal });
      checkAbort(controller.signal);
      const url = URL.createObjectURL(blob); let disposed = false;
      return { job, blob, url, dispose() { if (!disposed) { disposed = true; URL.revokeObjectURL(url); } } };
    } catch (error) {
      // A fresh bounded request is necessary: the generation signal may already be aborted.
      if (!['failed', 'cancelled'].includes(job?.status)) void this.cancel(job?.id || requestId).catch(() => {});
      throw error;
    } finally { clearTimeout(timer); signal?.removeEventListener('abort', abort); }
  }
}
