// <oxdm-stream src="ws://127.0.0.1:PORT/ws/ID"> — live player for the native
// RTSP backend (src/video/rtsp.rs). The WebSocket delivers one JSON "init"
// message, then binary frames: [kind u8][flags u8][ts_us u64 LE][payload].
// kind 0 = video (AVCC access unit, flags&1 = key), 1 = audio (s16le PCM).
//
// Video is decoded with WebCodecs and painted to a <canvas>. Decoded frames
// are not shown the instant they land: RTP over TCP arrives in bursts, and
// painting on arrival turns a steady 15 fps source into visible judder. A
// small presentation queue paced by requestAnimationFrame against the frame
// timestamps (PRESENT_DELAY_MS behind arrival) smooths that out — the same
// thing every native player does. Where WebCodecs is missing (older
// WebKitGTK) the element swaps in an <img> on the server's OpenH264→MJPEG
// route instead.
//
// Audio starts off; the server only sends it after we ask. PCM is scheduled
// onto an AudioContext with a small cushion.
const PRESENT_DELAY_MS = 150;   // jitter buffer: ~2 frames at 15 fps
const MAX_QUEUE = 12;           // burst cap before we re-anchor the clock

// Feather-style stroke icons; colour comes from the button's `currentColor`.
const svg = (body) => `<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${body}</svg>`;
const ICON_PLAY = svg('<polygon points="6 3 20 12 6 21 6 3"/>');
const ICON_PAUSE = svg('<rect x="6" y="4" width="4" height="16"/><rect x="14" y="4" width="4" height="16"/>');
const ICON_SOUND = svg('<polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><path d="M15.54 8.46a5 5 0 0 1 0 7.07"/><path d="M19.07 4.93a10 10 0 0 1 0 14.14"/>');
const ICON_MUTED = svg('<polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><line x1="23" y1="9" x2="17" y2="15"/><line x1="17" y1="9" x2="23" y2="15"/>');

class OxdmStream extends HTMLElement {
    static get observedAttributes() { return ['src']; }

    constructor() {
        super();
        this.muted = true;
        this.paused = false;
        this.live = false;
        this.audioNext = 0;
        this.queue = [];
        this.anchor = null;
        this.rafId = 0;
        this.stats = { since: performance.now(), received: 0, painted: 0 };
    }

    connectedCallback() {
        if (!this.canvas) this.build();
        this.connect();
    }

    disconnectedCallback() {
        this.teardown();
        if (this.audioCtx) { this.audioCtx.close(); this.audioCtx = null; }
    }

    attributeChangedCallback(name) {
        if (name === 'src' && this.canvas) { this.teardown(); this.connect(); }
    }

    // ── DOM ──────────────────────────────────────────────────────────────

    build() {
        this.canvas = document.createElement('canvas');
        this.canvas.className = 'oxdm-stream-canvas';
        this.ctx2d = this.canvas.getContext('2d', { alpha: false, desynchronized: true });
        this.appendChild(this.canvas);

        this.bar = document.createElement('div');
        this.bar.className = 'oxdm-live-bar';

        this.playBtn = document.createElement('button');
        this.playBtn.type = 'button';
        this.playBtn.addEventListener('click', () => {
            this.paused = !this.paused;
            if (this.paused) this.teardown(); else this.connect();
            this.refreshBar();
        });

        const badge = document.createElement('span');
        badge.className = 'oxdm-live-badge';
        badge.textContent = 'LIVE';

        this.muteBtn = document.createElement('button');
        this.muteBtn.type = 'button';
        this.muteBtn.addEventListener('click', () => {
            this.muted = !this.muted;
            // WebKit only honours AudioContext creation/resume inside a user
            // gesture; doing it from the WebSocket callback leaves it suspended.
            if (!this.muted) {
                if (!this.audioCtx) this.audioCtx = new (window.AudioContext || window.webkitAudioContext)();
                if (this.audioCtx.state === 'suspended') this.audioCtx.resume();
            }
            this.sendAudioPref();
            this.audioNext = 0;
            this.refreshBar();
        });

        this.bar.append(this.playBtn, badge, this.muteBtn);
        // Frameless PiP starts a window drag on mousedown; keep clicks on
        // the controls from doing that.
        this.bar.addEventListener('mousedown', (e) => e.stopPropagation());
        this.appendChild(this.bar);
        this.refreshBar();
    }

    refreshBar() {
        this.playBtn.innerHTML = this.paused ? ICON_PLAY : ICON_PAUSE;
        this.playBtn.title = this.paused ? 'Resume live' : 'Pause';
        this.muteBtn.innerHTML = this.muted ? ICON_MUTED : ICON_SOUND;
        this.muteBtn.title = this.muted ? 'Unmute' : 'Mute';
        this.muteBtn.disabled = !this.hasAudio;
        this.classList.toggle('oxdm-stream--paused', this.paused);
        this.classList.toggle('oxdm-stream--live', this.live && !this.paused);
    }

    // ── Connection ───────────────────────────────────────────────────────

    connect() {
        const url = this.getAttribute('src');
        if (!url || this.ws || this.paused || !this.isConnected) return;
        clearTimeout(this.reconnectTID);
        this.waitKey = true;
        const ws = new WebSocket(url);
        ws.binaryType = 'arraybuffer';
        ws.onmessage = ev => {
            if (typeof ev.data === 'string') this.onInit(JSON.parse(ev.data));
            else this.onFrame(ev.data);
        };
        ws.onclose = () => {
            if (this.ws !== ws) return;
            this.ws = null;
            this.setLive(false);
            if (!this.paused && this.isConnected) {
                this.reconnectTID = setTimeout(() => this.connect(), 2000);
            }
        };
        ws.onerror = () => ws.close();
        this.ws = ws;
    }

    teardown() {
        clearTimeout(this.reconnectTID);
        if (this.ws) { const ws = this.ws; this.ws = null; ws.onclose = null; ws.close(); }
        if (this.decoder) { try { this.decoder.close(); } catch (_) {} this.decoder = null; }
        if (this.img) { this.img.removeAttribute('src'); }
        this.flushQueue();
        this.setLive(false);
    }

    flushQueue() {
        for (const f of this.queue) f.close();
        this.queue = [];
        this.anchor = null;
        if (this.rafId) { cancelAnimationFrame(this.rafId); this.rafId = 0; }
    }

    setLive(v) {
        if (this.live === v) return;
        this.live = v;
        this.refreshBar();
    }

    sendAudioPref() {
        if (this.ws && this.ws.readyState === WebSocket.OPEN) {
            this.ws.send(JSON.stringify({ type: 'audio', enabled: !this.muted && !!this.hasAudio }));
        }
    }

    // ── Init / codec setup ───────────────────────────────────────────────

    onInit(init) {
        if (init.type !== 'init') return;
        this.hasAudio = !!init.audio;
        this.audioInfo = init.audio;
        this.sendAudioPref();
        this.refreshBar();

        if (this.img) return; // already on the MJPEG fallback
        if (!('VideoDecoder' in window)) return this.fallback('WebCodecs unavailable');

        const cfg = {
            codec: init.video.codec,
            description: base64ToBytes(init.video.description),
            codedWidth: init.video.width,
            codedHeight: init.video.height,
            optimizeForLatency: true,
            hardwareAcceleration: 'prefer-hardware',
        };
        const ws = this.ws;
        VideoDecoder.isConfigSupported(cfg).then(res => {
            if (this.ws !== ws) return;
            if (res.supported) return this.startDecoder(cfg);
            // Software path for the odd profile the GPU decoder refuses.
            cfg.hardwareAcceleration = 'no-preference';
            return VideoDecoder.isConfigSupported(cfg).then(r2 => {
                if (this.ws !== ws) return;
                if (!r2.supported) return this.fallback('codec ' + cfg.codec + ' unsupported');
                this.startDecoder(cfg);
            });
        }).catch(e => this.fallback(String(e)));
    }

    startDecoder(cfg) {
        if (this.decoder) { try { this.decoder.close(); } catch (_) {} }
        this.flushQueue();
        this.decoder = new VideoDecoder({
            output: frame => this.enqueue(frame),
            error: e => {
                console.error('[oxdm-stream] decoder error', e);
                // Recover by reconnecting; the server resends init + key frame.
                if (this.ws) this.ws.close();
            },
        });
        this.decoder.configure(cfg);
        this.waitKey = true;
    }

    fallback(reason) {
        console.warn('[oxdm-stream] falling back to MJPEG:', reason);
        if (this.decoder) { try { this.decoder.close(); } catch (_) {} this.decoder = null; }
        const src = this.getAttribute('src');
        const http = 'http' + src.slice(2).replace('/ws/', '/mjpeg/');
        if (!this.img) {
            this.img = document.createElement('img');
            this.img.className = 'oxdm-stream-canvas';
            this.img.addEventListener('load', () => {
                this.setLive(true);
                this.reportSize(this.img.naturalWidth, this.img.naturalHeight);
            });
            this.canvas.replaceWith(this.img);
        }
        this.img.src = http;
        this.hasAudio = false; // the <img> path carries no audio
        this.refreshBar();
    }

    // ── Frames ───────────────────────────────────────────────────────────

    onFrame(buf) {
        const dv = new DataView(buf);
        const kind = dv.getUint8(0);
        const flags = dv.getUint8(1);
        const ts = Number(dv.getBigUint64(2, true));
        const payload = new Uint8Array(buf, 10);
        if (kind === 0) this.onVideo(flags & 1, ts, payload);
        else if (kind === 1 && !this.muted) this.onAudio(payload);
    }

    onVideo(key, ts, data) {
        if (!this.decoder || this.decoder.state !== 'configured') return;
        if (this.waitKey && !key) return;
        this.waitKey = false;
        // Never gate deltas on decodeQueueSize: skipping to the next key frame
        // blanks the picture for a whole GOP (~2 s here). Latency is bounded
        // by the presentation queue trimming in enqueue() instead.
        this.stats.received++;
        try {
            this.decoder.decode(new EncodedVideoChunk({ type: key ? 'key' : 'delta', timestamp: ts, data }));
        } catch (e) {
            console.warn('[oxdm-stream] decode() threw', e);
            this.waitKey = true;
        }
    }

    // ── Presentation ───────────────────────────────────────────────────────

    enqueue(frame) {
        this.queue.push(frame);
        if (this.queue.length > MAX_QUEUE) {
            // A burst (or a decoder catching up) — drop to the freshest few
            // and re-anchor so latency doesn't ratchet upward.
            while (this.queue.length > 4) this.queue.shift().close();
            this.anchor = null;
        }
        if (!this.rafId) this.rafId = requestAnimationFrame(t => this.present(t));
    }

    present(nowMs) {
        this.rafId = 0;
        if (!this.queue.length) return;
        if (!this.anchor) {
            // Map media time onto the wall clock so the head frame shows
            // PRESENT_DELAY_MS from now; later frames follow at source cadence.
            this.anchor = { wall: nowMs + PRESENT_DELAY_MS, media: this.queue[0].timestamp / 1000 };
        }
        const mediaNow = (nowMs - this.anchor.wall) + this.anchor.media;
        let pick = null;
        while (this.queue.length && this.queue[0].timestamp / 1000 <= mediaNow) {
            if (pick) pick.close();
            pick = this.queue.shift();
        }
        if (pick) { this.paint(pick); }
        // Timestamp discontinuity (camera clock jump, reconnect): re-anchor
        // rather than freeze or fast-forward.
        if (this.queue.length) {
            const gap = this.queue[0].timestamp / 1000 - mediaNow;
            if (gap > 1000 || gap < -1000) this.anchor = null;
            this.rafId = requestAnimationFrame(t => this.present(t));
        }
    }

    paint(frame) {
        // Draw at the size actually shown, not the source size: a 2K frame
        // painted into a 2K canvas and then CSS-scaled costs several times
        // more fill than scaling once here, and it all lands on the main thread.
        const dpr = window.devicePixelRatio || 1;
        const boxW = Math.max(1, Math.round(this.clientWidth * dpr));
        const boxH = Math.max(1, Math.round(this.clientHeight * dpr));
        const scale = Math.min(1, boxW / frame.displayWidth, boxH / frame.displayHeight);
        const w = Math.max(1, Math.round(frame.displayWidth * scale));
        const h = Math.max(1, Math.round(frame.displayHeight * scale));
        if (this.canvas.width !== w || this.canvas.height !== h) {
            this.canvas.width = w;
            this.canvas.height = h;
        }
        this.ctx2d.drawImage(frame, 0, 0, w, h);
        this.reportSize(frame.displayWidth, frame.displayHeight);
        frame.close();
        this.stats.painted++;
        this.setLive(true);
        this.maybeLogStats();
    }

    // Bubbles so hosts (the PiP window) can size themselves to the source.
    reportSize(width, height) {
        if (!width || !height || (width === this.srcW && height === this.srcH)) return;
        this.srcW = width; this.srcH = height;
        this.dispatchEvent(new CustomEvent('oxdm-videosize', { bubbles: true, detail: { width, height } }));
    }

    maybeLogStats() {
        const now = performance.now();
        if (now - this.stats.since < 5000) return;
        const s = (now - this.stats.since) / 1000;
        console.debug(`[oxdm-stream] rx ${(this.stats.received / s).toFixed(1)} fps, painted ${(this.stats.painted / s).toFixed(1)} fps, queue ${this.queue.length}, decodeQueue ${this.decoder ? this.decoder.decodeQueueSize : '-'}`);
        this.stats = { since: now, received: 0, painted: 0 };
    }

    onAudio(bytes) {
        const info = this.audioInfo;
        const ctx = this.audioCtx;
        if (!info || !ctx || ctx.state !== 'running') return;
        const ch = info.channels || 1;
        const pcm = new Int16Array(bytes.slice().buffer);
        const frames = Math.floor(pcm.length / ch);
        if (frames === 0) return;
        const buf = ctx.createBuffer(ch, frames, info.sample_rate);
        for (let c = 0; c < ch; c++) {
            const out = buf.getChannelData(c);
            for (let i = 0; i < frames; i++) out[i] = pcm[i * ch + c] / 32768;
        }
        const now = ctx.currentTime;
        // Keep ~120 ms of cushion; if we drift past 600 ms (stall, tab hidden)
        // snap back to live rather than play stale audio.
        if (this.audioNext < now + 0.02 || this.audioNext > now + 0.6) this.audioNext = now + 0.12;
        const src = ctx.createBufferSource();
        src.buffer = buf;
        src.connect(ctx.destination);
        src.start(this.audioNext);
        this.audioNext += buf.duration;
    }
}

function base64ToBytes(b64) {
    const bin = atob(b64);
    const out = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
    return out;
}

customElements.define('oxdm-stream', OxdmStream);
