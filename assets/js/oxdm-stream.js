// <oxdm-stream src="ws://127.0.0.1:PORT/ws/ID"> — live player for the native
// RTSP backend (src/video/rtsp.rs). The WebSocket delivers one JSON "init"
// message, then binary frames: [kind u8][flags u8][ts_us u64 LE][payload].
// kind 0 = video (AVCC access unit, flags&1 = key), 1 = audio (s16le PCM).
// A JSON "error" message reports a failing camera session until the next init.
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

class WebGlVideoRenderer {
    constructor(canvas) {
        const gl = canvas.getContext('webgl2', {
            alpha: false, antialias: false, depth: false, stencil: false,
            // Keep the last picture visible while playback is paused.
            preserveDrawingBuffer: true,
        });
        if (!gl) throw new Error('WebGL2 unavailable');
        this.gl = gl;
        const shaders = [];
        try {
            this.program = gl.createProgram();
            if (!this.program) throw new Error('WebGL program allocation failed');
            for (const [type, source] of [
                [gl.VERTEX_SHADER, `#version 300 es
                    out vec2 uv;
                    void main() {
                        vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
                        uv = vec2(p.x, 1.0 - p.y);
                        gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
                    }`],
                [gl.FRAGMENT_SHADER, `#version 300 es
                    precision highp float;
                    uniform sampler2D video;
                    in vec2 uv;
                    out vec4 color;
                    void main() { color = texture(video, uv); }`],
            ]) {
                const shader = gl.createShader(type);
                if (!shader) throw new Error('WebGL shader allocation failed');
                shaders.push(shader);
                gl.shaderSource(shader, source);
                gl.compileShader(shader);
                if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
                    throw new Error(gl.getShaderInfoLog(shader) || 'WebGL shader compilation failed');
                }
                gl.attachShader(this.program, shader);
            }
            gl.linkProgram(this.program);
            if (!gl.getProgramParameter(this.program, gl.LINK_STATUS)) {
                throw new Error(gl.getProgramInfoLog(this.program) || 'WebGL program linking failed');
            }
            gl.useProgram(this.program);
            this.texture = gl.createTexture();
            if (!this.texture) throw new Error('WebGL texture allocation failed');
            gl.activeTexture(gl.TEXTURE0);
            gl.bindTexture(gl.TEXTURE_2D, this.texture);
            gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
            gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
            gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
            gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
            gl.uniform1i(gl.getUniformLocation(this.program, 'video'), 0);
        } catch (error) {
            this.dispose();
            throw error;
        } finally {
            for (const shader of shaders) gl.deleteShader(shader);
        }
    }

    paint(frame, width, height) {
        const gl = this.gl;
        if (gl.isContextLost()) throw new Error('WebGL context lost');
        gl.viewport(0, 0, width, height);
        gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, frame);
        // Probe upload compatibility once per source size, not a GPU round-trip every frame.
        if (this.frameWidth !== frame.codedWidth || this.frameHeight !== frame.codedHeight) {
            const error = gl.getError();
            if (error !== gl.NO_ERROR) throw new Error(`WebGL frame upload failed: ${error}`);
            this.frameWidth = frame.codedWidth;
            this.frameHeight = frame.codedHeight;
        }
        gl.drawArrays(gl.TRIANGLES, 0, 3);
    }

    dispose() {
        if (this.texture) this.gl.deleteTexture(this.texture);
        if (this.program) this.gl.deleteProgram(this.program);
        this.texture = this.program = null;
    }
}

// Feather-style stroke icons; colour comes from the button's `currentColor`.
const svg = (body) => `<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${body}</svg>`;
const ICON_PLAY = svg('<polygon points="6 3 20 12 6 21 6 3"/>');
const ICON_PAUSE = svg('<rect x="6" y="4" width="4" height="16"/><rect x="14" y="4" width="4" height="16"/>');
const ICON_SOUND = svg('<polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><path d="M15.54 8.46a5 5 0 0 1 0 7.07"/><path d="M19.07 4.93a10 10 0 0 1 0 14.14"/>');
const ICON_MUTED = svg('<polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><line x1="23" y1="9" x2="17" y2="15"/><line x1="17" y1="9" x2="23" y2="15"/>');
const ICON_ALERT = svg('<path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3"/><path d="M12 9v4"/><path d="M12 17h.01"/>');

class OxdmStream extends HTMLElement {
    static get observedAttributes() { return ['src', 'data-labels']; }

    constructor() {
        super();
        this.muted = true;
        this.paused = false;
        this.live = false;
        this.decodePath = 'waiting';
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
        this.releaseRenderer();
        if (this.audioCtx) { this.audioCtx.close(); this.audioCtx = null; }
    }

    attributeChangedCallback(name) {
        if (name === 'src' && this.canvas) {
            this.teardown();
            this.connect();
        }
        if (name === 'data-labels' && this.decodeStatus) this.refreshDecodeStatus();
    }

    // ── DOM ──────────────────────────────────────────────────────────────

    build() {
        this.canvas = document.createElement('canvas');
        this.canvas.className = 'oxdm-stream-canvas';
        this.appendChild(this.canvas);
        this.canvas.addEventListener('webglcontextlost', () => {
            if (this.renderer) this.useCanvas2d('WebGL context lost');
        });

        this.decodeIndicator = document.createElement('div');
        this.decodeIndicator.className = 'video-decode-status';
        this.decodeStatus = document.createElement('span');
        this.decodeStatus.setAttribute('role', 'status');
        this.decodeIndicator.append(this.decodeStatus);
        this.appendChild(this.decodeIndicator);

        this.errorEl = document.createElement('div');
        this.errorEl.className = 'oxdm-stream-error';
        this.errorEl.hidden = true;
        this.errorEl.innerHTML = ICON_ALERT;
        this.errorText = this.errorEl.appendChild(document.createElement('span'));
        this.appendChild(this.errorEl);

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

        this.zoomLabel = document.createElement('span');
        this.zoomLabel.className = 'oxdm-live-zoom';

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

        this.bar.append(this.playBtn, badge, this.zoomLabel, this.muteBtn);
        // Frameless PiP starts a window drag on mousedown; keep clicks on
        // the controls from doing that.
        for (const button of [this.playBtn, this.muteBtn]) {
            button.addEventListener('mousedown', (event) => event.stopPropagation());
        }
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
        this.refreshDecodeStatus();
    }

    setDecodePath(path) {
        this.decodePath = path;
        this.refreshDecodeStatus();
    }

    refreshDecodeStatus() {
        const labels = JSON.parse(this.getAttribute('data-labels'));
        const path = this.paused ? 'paused' : this.decodePath;
        const status = labels[path];
        this.decodeStatus.textContent = status.label;
        this.decodeStatus.title = status.title;
        this.decodeStatus.setAttribute('aria-label', status.title);
        this.decodeIndicator.classList.toggle('video-decode-status--software', path === 'software-mjpeg');
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
            if (typeof ev.data !== 'string') return this.onFrame(ev.data);
            const msg = JSON.parse(ev.data);
            if (msg.type === 'error') this.showError(msg.message);
            else this.onInit(msg);
        };
        ws.onclose = () => {
            if (this.ws !== ws) return;
            this.ws = null;
            this.setLive(false);
            if (!this.img) this.setDecodePath('waiting');
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
        this.setDecodePath('waiting');
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

    sendVideoPref() {
        if (this.ws && this.ws.readyState === WebSocket.OPEN) {
            this.ws.send(JSON.stringify({ type: 'video', enabled: !this.img }));
        }
    }

    showError(message) {
        this.errorText.textContent = message;
        this.errorEl.hidden = !message;
    }

    // ── Init / codec setup ───────────────────────────────────────────────

    onInit(init) {
        if (init.type !== 'init') return;
        this.showError('');
        this.hasAudio = !!init.audio;
        this.audioInfo = init.audio;
        this.sendAudioPref();
        this.sendVideoPref();
        this.refreshBar();

        if (this.img) {
            this.img.src = this.mjpegUrl();
            this.setDecodePath('software-mjpeg');
            return;
        }
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
        }).catch(e => {
            if (this.ws === ws) this.fallback(String(e));
        });
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
        this.setDecodePath(cfg.hardwareAcceleration === 'prefer-hardware' ? 'hardware-preferred' : 'automatic');
    }

    mjpegUrl() {
        return 'http' + this.getAttribute('src').slice(2).replace('/ws/', '/mjpeg/');
    }

    fallback(reason) {
        console.warn('[oxdm-stream] falling back to MJPEG:', reason);
        if (this.decoder) { try { this.decoder.close(); } catch (_) {} this.decoder = null; }
        this.flushQueue();
        this.releaseRenderer();
        this.setAttribute('data-renderer-active', 'mjpeg');
        if (!this.img) {
            this.img = document.createElement('img');
            this.img.className = 'oxdm-stream-canvas';
            this.img.addEventListener('load', () => {
                this.setLive(true);
                this.reportSize(this.img.naturalWidth, this.img.naturalHeight);
            });
            this.canvas.replaceWith(this.img);
        }
        this.img.src = this.mjpegUrl();
        this.setDecodePath('software-mjpeg');
        this.sendVideoPref();
        // Only video falls back to MJPEG; PCM audio still arrives over the WebSocket.
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

    releaseRenderer() {
        if (this.renderer) this.renderer.dispose();
        this.renderer = null;
    }

    useCanvas2d(reason) {
        console.warn('[oxdm-stream] using Canvas 2D:', reason);
        this.releaseRenderer();
        this.webglFailed = true;
        // A canvas cannot switch context types. Preserve its geometry and digital zoom.
        const old = this.canvas;
        const canvas = document.createElement('canvas');
        canvas.className = old.className;
        canvas.width = old.width;
        canvas.height = old.height;
        canvas.style.cssText = old.style.cssText;
        const zoom = zoomState.get(old);
        if (zoom) {
            zoomState.set(canvas, zoom);
            zoomState.delete(old);
        }
        old.replaceWith(canvas);
        this.canvas = canvas;
        this.ctx2d = null;
    }

    drawFrame(frame, width, height) {
        if (!this.ctx2d && !this.webglFailed && (this.getAttribute('data-renderer') ?? 'webgl2') === 'webgl2') {
            try {
                if (!this.renderer) this.renderer = new WebGlVideoRenderer(this.canvas);
                this.renderer.paint(frame, width, height);
                if (this.getAttribute('data-renderer-active') !== 'webgl2') {
                    this.setAttribute('data-renderer-active', 'webgl2');
                }
                return;
            } catch (error) {
                this.useCanvas2d(String(error));
            }
        }
        if (!this.ctx2d) this.ctx2d = this.canvas.getContext('2d', { alpha: false, desynchronized: true });
        if (!this.ctx2d) throw new Error('Canvas 2D unavailable');
        this.ctx2d.drawImage(frame, 0, 0, width, height);
        if (this.getAttribute('data-renderer-active') !== '2d') {
            this.setAttribute('data-renderer-active', '2d');
        }
    }

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
        // Digital zoom is a CSS scale on the canvas, so it enlarges what is shown.
        const zoom = zoomState.get(this.canvas);
        const dpr = (window.devicePixelRatio || 1) * (zoom ? zoom.s : 1);
        const boxW = Math.max(1, Math.round(this.clientWidth * dpr));
        const boxH = Math.max(1, Math.round(this.clientHeight * dpr));
        const scale = Math.min(1, boxW / frame.displayWidth, boxH / frame.displayHeight);
        const w = Math.max(1, Math.round(frame.displayWidth * scale));
        const h = Math.max(1, Math.round(frame.displayHeight * scale));
        if (this.canvas.width !== w || this.canvas.height !== h) {
            this.canvas.width = w;
            this.canvas.height = h;
        }
        try {
            this.drawFrame(frame, w, h);
            this.reportSize(frame.displayWidth, frame.displayHeight);
        } finally {
            frame.close();
        }
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
        console.debug(`[oxdm-stream] renderer ${this.getAttribute('data-renderer-active')}, rx ${(this.stats.received / s).toFixed(1)} fps, painted ${(this.stats.painted / s).toFixed(1)} fps, queue ${this.queue.length}, decodeQueue ${this.decoder ? this.decoder.decodeQueueSize : '-'}`);
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

// ── Digital zoom ─────────────────────────────────────────────────────────
// Pinch zooms any .live-video-frame around the pointer; two-finger scroll pans
// it while zoomed. Only the picture is transformed — the camera is not asked
// to zoom. WebKit reports a trackpad pinch as gesture* events, Chromium as
// ctrl+wheel.
const MAX_ZOOM = 8;
const zoomState = new WeakMap(); // picture element -> { s, x, y }
let pinch = null;                // { media, scale } during a WebKit gesture

// The element holding the pixels: a plain <img> frame, or the player's
// canvas/<img> inside <oxdm-stream>.
function zoomTarget(node) {
    const frame = node instanceof Element ? node.closest('.live-video-frame') : null;
    if (!frame) return null;
    return frame.matches('img') ? frame : frame.querySelector('.oxdm-stream-canvas');
}

function setZoom(media, s, x, y) {
    const w = media.clientWidth, h = media.clientHeight;
    if (s <= 1 || !w || !h) {
        zoomState.delete(media);
        media.style.transform = '';
        return;
    }
    // object-fit: contain letterboxes the picture, so clamp against the
    // picture rather than the element box.
    const nw = media.naturalWidth || media.width, nh = media.naturalHeight || media.height;
    const fit = Math.min(w / nw, h / nh);
    x = clampPan(x, s, w, nw * fit);
    y = clampPan(y, s, h, nh * fit);
    zoomState.set(media, { s, x, y });
    media.style.transformOrigin = '0 0';
    media.style.transform = `translate(${x}px, ${y}px) scale(${s})`;
}

// Keep the scaled picture covering the box on this axis, or centred while
// it is still smaller than the box.
function clampPan(t, s, box, pic) {
    const inset = (box - pic) / 2 * s;
    if (pic * s <= box) return (box - pic * s) / 2 - inset;
    return Math.min(-inset, Math.max(box - inset - pic * s, t));
}

function zoomBy(media, factor, clientX, clientY) {
    const cur = zoomState.get(media) || { s: 1, x: 0, y: 0 };
    const s = Math.min(MAX_ZOOM, Math.max(1, cur.s * factor));
    const k = s / cur.s;
    // Pointer in untransformed element coordinates; the point under it stays put.
    const r = media.getBoundingClientRect();
    const px = clientX - r.left + cur.x, py = clientY - r.top + cur.y;
    setZoom(media, s, px - (px - cur.x) * k, py - (py - cur.y) * k);
    const player = media.closest('oxdm-stream');
    if (player) player.zoomLabel.textContent = s > 1 ? `${s.toFixed(2)}x` : '';
}

document.addEventListener('wheel', e => {
    const media = zoomTarget(e.target);
    if (!media) return;
    const cur = zoomState.get(media);
    if (e.ctrlKey) {
        e.preventDefault();
        zoomBy(media, Math.exp(-e.deltaY / 100), e.clientX, e.clientY);
    } else if (cur) {
        e.preventDefault();
        setZoom(media, cur.s, cur.x - e.deltaX, cur.y - e.deltaY);
    }
}, { passive: false });

document.addEventListener('gesturestart', e => {
    const media = zoomTarget(e.target);
    if (!media) return;
    e.preventDefault();
    pinch = { media, scale: 1 };
});
document.addEventListener('gesturechange', e => {
    if (!pinch) return;
    e.preventDefault();
    // e.scale is cumulative since gesturestart; apply only the step.
    zoomBy(pinch.media, e.scale / pinch.scale, e.clientX, e.clientY);
    pinch.scale = e.scale;
});
document.addEventListener('gestureend', () => { pinch = null; });

customElements.define('oxdm-stream', OxdmStream);
