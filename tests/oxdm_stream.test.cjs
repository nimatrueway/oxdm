const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const { test } = require('node:test');
const vm = require('node:vm');

class Element {
    constructor() {
        this.attributes = new Map();
        this.classes = new Set();
        this.classList = {
            toggle: (name, enabled) => enabled ? this.classes.add(name) : this.classes.delete(name),
        };
        this.children = [];
        this.listeners = new Map();
        this.isConnected = true;
        this.style = {};
        this.clientWidth = 640;
        this.clientHeight = 360;
    }
    setAttribute(name, value) { this.attributes.set(name, value); }
    getAttribute(name) { return this.attributes.get(name) ?? null; }
    removeAttribute(name) { this.attributes.delete(name); }
    set src(value) { this.setAttribute('src', value); }
    get src() { return this.getAttribute('src') ?? ''; }
    appendChild(child) { this.children.push(child); return child; }
    append(...children) { this.children.push(...children); }
    addEventListener(name, listener) { this.listeners.set(name, listener); }
    getContext() { return {}; }
    replaceWith(element) { this.replacement = element; }
    dispatchEvent() {}
}

function setup(support, { renderer, gl = null } = {}) {
    let Player;
    const draws = [];
    const context = {
        HTMLElement: Element,
        Element,
        document: {
            createElement: tag => {
                const element = new Element();
                if (tag === 'canvas') {
                    let acquired;
                    element.getContext = type => {
                        if (acquired && acquired !== type) return null;
                        const result = type === 'webgl2' ? gl : { drawImage: (...args) => draws.push(args) };
                        if (result) acquired = type;
                        return result;
                    };
                }
                return element;
            },
            addEventListener() {},
        },
        customElements: { define: (_, value) => { Player = value; } },
        performance,
        console: { warn() {}, error() {}, debug() {} },
        clearTimeout,
        setTimeout,
        atob,
        cancelAnimationFrame() {},
        CustomEvent: class {},
        window: {},
        WebSocket: class {
            static OPEN = 1;
            constructor(url) { this.url = url; this.readyState = 1; this.sent = []; }
            send(message) { this.sent.push(JSON.parse(message)); }
            close() {}
        },
    };
    if (support) {
        context.VideoDecoder = class {
            static async isConfigSupported(config) { return { supported: support(config) }; }
            configure(config) { this.config = config; this.state = 'configured'; }
            close() { this.state = 'closed'; }
        };
        context.window.VideoDecoder = context.VideoDecoder;
    }
    vm.runInNewContext(readFileSync(`${__dirname}/../assets/js/oxdm-stream.js`, 'utf8'), context);
    const player = new Player();
    const labels = Object.fromEntries(
        ['waiting', 'paused', 'hardware-preferred', 'automatic', 'software-mjpeg']
            .map(path => [path, { label: `localized ${path}`, title: `hint ${path}` }]),
    );
    labels.dismiss = 'localized close';
    player.setAttribute('src', 'ws://127.0.0.1:1234/ws/test');
    player.setAttribute('data-labels', JSON.stringify(labels));
    if (renderer) player.setAttribute('data-renderer', renderer);
    player.connectedCallback();
    player.canvasDraws = draws;
    return player;
}

function mockGl({ compile = true, link = true, uploadError = 0, uploadThrows = false } = {}) {
    const calls = [];
    const gl = { calls };
    for (const [i, key] of [
        'VERTEX_SHADER', 'FRAGMENT_SHADER', 'COMPILE_STATUS', 'LINK_STATUS',
        'TEXTURE0', 'TEXTURE_2D', 'TEXTURE_MIN_FILTER', 'TEXTURE_MAG_FILTER',
        'TEXTURE_WRAP_S', 'TEXTURE_WRAP_T', 'LINEAR', 'CLAMP_TO_EDGE',
        'RGBA', 'UNSIGNED_BYTE', 'TRIANGLES',
    ].entries()) gl[key] = i + 1;
    gl.NO_ERROR = 0;
    for (const name of [
        'shaderSource', 'compileShader', 'attachShader', 'linkProgram', 'useProgram',
        'activeTexture', 'bindTexture', 'texParameteri', 'uniform1i', 'viewport',
        'drawArrays', 'deleteShader', 'deleteTexture', 'deleteProgram',
    ]) gl[name] = (...args) => calls.push([name, ...args]);
    gl.createProgram = () => ({ type: 'program' });
    gl.createShader = type => ({ type });
    gl.createTexture = () => ({ type: 'texture' });
    gl.getShaderParameter = () => compile;
    gl.getProgramParameter = () => link;
    gl.getShaderInfoLog = gl.getProgramInfoLog = () => 'test shader failure';
    gl.getUniformLocation = () => ({ type: 'uniform' });
    gl.isContextLost = () => false;
    gl.getError = () => { calls.push(['getError']); return uploadError; };
    gl.texImage2D = (...args) => {
        calls.push(['texImage2D', ...args]);
        if (uploadThrows) throw new TypeError('VideoFrame uploads unavailable');
    };
    return gl;
}

function videoFrame() {
    return {
        displayWidth: 1920, displayHeight: 1080, codedWidth: 1920, codedHeight: 1088,
        closed: 0, close() { this.closed++; },
    };
}

test('explicit Canvas 2D override bypasses WebGL and frames are scaled and closed', () => {
    const gl = mockGl();
    const player = setup(() => true, { renderer: '2d', gl });
    const frame = videoFrame();
    player.paint(frame);
    assert.equal(player.getAttribute('data-renderer-active'), '2d');
    assert.deepEqual(player.canvasDraws[0], [frame, 0, 0, 640, 360]);
    assert.equal(frame.closed, 1);
    assert.equal(gl.calls.length, 0);
});

for (const renderer of [undefined, 'webgl2']) {
    test(`${renderer ?? 'default'} WebGL uploads VideoFrames directly, draws a triangle, and reuses resources`, () => {
        const gl = mockGl();
        const player = setup(() => true, { renderer, gl });
        const frame = videoFrame();
        player.paint(frame);
        const instance = player.renderer;
        player.paint(videoFrame());
        assert.equal(player.renderer, instance);
        assert.equal(player.getAttribute('data-renderer-active'), 'webgl2');
        assert.equal(player.canvasDraws.length, 0);
        assert.equal(gl.calls.find(c => c[0] === 'texImage2D').at(-1), frame);
        assert.deepEqual(gl.calls.find(c => c[0] === 'viewport'), ['viewport', 0, 0, 640, 360]);
        assert.deepEqual(gl.calls.find(c => c[0] === 'drawArrays'), ['drawArrays', gl.TRIANGLES, 0, 3]);
        assert.equal(gl.calls.filter(c => c[0] === 'getError').length, 1);
        assert.equal(gl.calls.filter(c => c[0] === 'linkProgram').length, 1);
        assert.equal(frame.closed, 1);
        player.paint({ ...videoFrame(), codedWidth: 1280, codedHeight: 720 });
        assert.equal(gl.calls.filter(c => c[0] === 'getError').length, 2);
    });
}

for (const [reason, gl] of [
    ['missing WebGL2', null],
    ['shader compilation failure', mockGl({ compile: false })],
    ['program linking failure', mockGl({ link: false })],
    ['unsupported VideoFrame uploads', mockGl({ uploadThrows: true })],
    ['upload GL error', mockGl({ uploadError: 1282 })],
]) {
    test(`${reason} falls back on a new 2D canvas without dropping the frame or decoder`, () => {
        const player = setup(() => true, { gl });
        const old = player.canvas;
        old.style.cssText = 'transform: scale(2)';
        const decoder = { state: 'configured' };
        player.decoder = decoder;
        const frame = videoFrame();
        player.paint(frame);
        assert.equal(old.replacement, player.canvas);
        assert.equal(player.canvas.style.cssText, old.style.cssText);
        assert.equal(player.canvas.width, 640);
        assert.equal(player.canvas.height, 360);
        assert.equal(player.getAttribute('data-renderer-active'), '2d');
        assert.equal(player.decoder, decoder);
        assert.equal(frame.closed, 1);
        assert.equal(player.canvasDraws.length, 1);
        player.paint(videoFrame());
        assert.equal(player.canvasDraws.length, 2);
        if (gl) assert.equal(gl.calls.filter(c => c[0] === 'deleteProgram').length, 1);
    });
}

test('context loss switches to 2D and releases WebGL resources', () => {
    const gl = mockGl();
    const player = setup(() => true, { renderer: 'webgl2', gl });
    player.paint(videoFrame());
    player.canvas.listeners.get('webglcontextlost')();
    player.paint(videoFrame());
    assert.equal(player.getAttribute('data-renderer-active'), '2d');
    assert.equal(gl.calls.filter(c => c[0] === 'deleteTexture').length, 1);
    assert.equal(gl.calls.filter(c => c[0] === 'deleteProgram').length, 1);
});

test('pause retains the WebGL picture; disconnect releases and reconnect recreates resources', () => {
    const gl = mockGl();
    const player = setup(() => true, { renderer: 'webgl2', gl });
    player.paint(videoFrame());
    const renderer = player.renderer;
    player.playBtn.listeners.get('click')();
    assert.equal(player.renderer, renderer);
    player.disconnectedCallback();
    assert.equal(player.renderer, null);
    assert.equal(gl.calls.filter(c => c[0] === 'deleteTexture').length, 1);
    player.connectedCallback();
    player.playBtn.listeners.get('click')();
    player.paint(videoFrame());
    assert.notEqual(player.renderer, renderer);
});

test('MJPEG fallback releases graphics resources without changing its audio path', () => {
    const gl = mockGl();
    const player = setup(() => true, { renderer: 'webgl2', gl });
    player.paint(videoFrame());
    player.fallback('test fallback');
    assert.equal(player.renderer, null);
    assert.equal(player.getAttribute('data-renderer-active'), 'mjpeg');
    assert.equal(gl.calls.filter(c => c[0] === 'deleteTexture').length, 1);
    assert.deepEqual(player.ws.sent.at(-1), { type: 'video', enabled: false });
});

test('a failed final draw still closes its VideoFrame', () => {
    const player = setup(() => true, { renderer: '2d' });
    player.canvas.getContext = () => null;
    const frame = videoFrame();
    assert.throws(() => player.paint(frame), /Canvas 2D unavailable/);
    assert.equal(frame.closed, 1);
});

const init = {
    type: 'init',
    video: { codec: 'avc1.64001F', description: '', width: 1920, height: 1080 },
};
const settled = () => new Promise(resolve => setImmediate(resolve));

test('hardware-preferred WebCodecs is labeled as a preference, not verified hardware', async () => {
    const player = setup(() => true);
    assert.equal(player.decodeStatus.textContent, 'localized waiting');
    player.onInit(init);
    await settled();
    assert.equal(player.decoder.config.hardwareAcceleration, 'prefer-hardware');
    assert.equal(player.decodeStatus.textContent, 'localized hardware-preferred');
    assert.equal(player.decodeStatus.title, 'hint hardware-preferred');
    assert.equal(player.decodeStatus.getAttribute('aria-label'), 'hint hardware-preferred');
    assert.equal(player.decodeIndicator.classes.has('video-decode-status--software'), false);
    assert.equal(player.decodeClose.getAttribute('aria-label'), 'localized close');
    assert.deepEqual(player.ws.sent.find(message => message.type === 'video'), { type: 'video', enabled: true });
});

test('unsupported hardware preference is labeled automatic, not software-only', async () => {
    const player = setup(config => config.hardwareAcceleration === 'no-preference');
    player.onInit(init);
    await settled();
    assert.equal(player.decoder.config.hardwareAcceleration, 'no-preference');
    assert.equal(player.decodeStatus.textContent, 'localized automatic');
});

for (const [reason, support] of [
    ['WebCodecs missing', null],
    ['codec unsupported', () => false],
    ['capability check rejected', () => { throw new Error('capability check failed'); }],
]) {
    test(`${reason} displays the software transcoding warning`, async () => {
        const player = setup(support);
        player.onInit(init);
        await settled();
        assert.equal(player.img.src, 'http://127.0.0.1:1234/mjpeg/test');
        assert.equal(player.decodeStatus.textContent, 'localized software-mjpeg');
        assert.equal(player.decodeIndicator.classes.has('video-decode-status--software'), true);
        assert.deepEqual(player.ws.sent.at(-1), { type: 'video', enabled: false });
    });
}

test('pause clears decoder status and resume restores the fallback image and warning', () => {
    const player = setup();
    player.onInit(init);
    player.playBtn.listeners.get('click')();
    assert.equal(player.decodeStatus.textContent, 'localized paused');
    assert.equal(player.img.getAttribute('src'), null);
    assert.equal(player.img.src, '');
    assert.equal(player.decodeIndicator.classes.has('video-decode-status--software'), false);
    player.playBtn.listeners.get('click')();
    assert.equal(player.decodeStatus.textContent, 'localized waiting');
    player.onInit(init);
    assert.equal(player.img.src, 'http://127.0.0.1:1234/mjpeg/test');
    assert.equal(player.decodeStatus.textContent, 'localized software-mjpeg');
    assert.deepEqual(player.ws.sent.find(message => message.type === 'video'), { type: 'video', enabled: false });
});

test('disconnect clears WebCodecs status and language changes update the badge', async () => {
    const player = setup(() => true);
    player.onInit(init);
    await settled();
    player.isConnected = false;
    player.ws.onclose();
    assert.equal(player.decodeStatus.textContent, 'localized waiting');
    player.setAttribute('data-labels', JSON.stringify({ waiting: { label: 'translated', title: 'translated hint' }, dismiss: 'translated close' }));
    player.attributeChangedCallback('data-labels');
    assert.equal(player.decodeStatus.textContent, 'translated');
    assert.equal(player.decodeStatus.title, 'translated hint');
    assert.equal(player.decodeClose.title, 'translated close');
});

test('a stale rejected capability check cannot activate fallback after teardown', async () => {
    const player = setup(() => { throw new Error('capability check failed'); });
    player.onInit(init);
    player.teardown();
    await settled();
    assert.equal(player.img, undefined);
    assert.equal(player.decodeStatus.textContent, 'localized waiting');
});

test('a stale successful capability check cannot restore decoder status after pause', async () => {
    const player = setup(() => true);
    player.onInit(init);
    player.playBtn.listeners.get('click')();
    await settled();
    assert.equal(player.decoder, undefined);
    assert.equal(player.decodeStatus.textContent, 'localized paused');
});

test('changing source removes the old fallback image until the new stream is initialized', () => {
    const player = setup();
    player.onInit(init);
    player.setAttribute('src', 'ws://127.0.0.1:1234/ws/new');
    player.attributeChangedCallback('src');
    assert.equal(player.img.src, '');
    assert.equal(player.decodeStatus.textContent, 'localized waiting');
    player.onInit(init);
    assert.equal(player.img.src, 'http://127.0.0.1:1234/mjpeg/new');
    assert.equal(player.decodeStatus.textContent, 'localized software-mjpeg');
});

test('MJPEG fallback still requests audio when unmuted', () => {
    const player = setup();
    player.onInit({ ...init, audio: { sample_rate: 8000, channels: 1 } });
    player.muted = false;
    player.sendAudioPref();
    assert.deepEqual(player.ws.sent.at(-1), { type: 'audio', enabled: true });
    assert.deepEqual(player.ws.sent.filter(message => message.type === 'video').at(-1), { type: 'video', enabled: false });
});

test('closing the badge leaves decoding connected and stays closed through pause/resume', async () => {
    const player = setup(() => true);
    player.onInit(init);
    await settled();
    const ws = player.ws;
    const decoder = player.decoder;
    let stopped = 0;
    player.decodeClose.listeners.get('mousedown')({ stopPropagation() { stopped++; } });
    player.decodeClose.listeners.get('click')({ stopPropagation() { stopped++; } });
    assert.equal(stopped, 2);
    assert.equal(player.decodeIndicator.hidden, true);
    assert.equal(player.ws, ws);
    assert.equal(player.decoder, decoder);
    player.playBtn.listeners.get('click')();
    player.playBtn.listeners.get('click')();
    player.onInit(init);
    await settled();
    assert.equal(player.decodeIndicator.hidden, true);
    assert.equal(player.decoder.state, 'configured');
});

test('closing the fallback badge does not stop its image and survives reconnects', () => {
    const player = setup();
    player.onInit(init);
    const imageUrl = player.img.src;
    player.decodeClose.listeners.get('click')({ stopPropagation() {} });
    assert.equal(player.img.src, imageUrl);
    player.teardown();
    player.connect();
    player.onInit(init);
    assert.equal(player.decodeIndicator.hidden, true);
    assert.equal(player.img.src, imageUrl);
});

test('a different stream restores the badge but status and label updates do not', () => {
    const player = setup();
    player.onInit(init);
    player.decodeClose.listeners.get('click')({ stopPropagation() {} });
    player.setDecodePath('waiting');
    player.attributeChangedCallback('data-labels');
    assert.equal(player.decodeIndicator.hidden, true);
    const oldUrl = player.getAttribute('src');
    player.attributeChangedCallback('src', oldUrl, oldUrl);
    assert.equal(player.decodeIndicator.hidden, true);
    const newUrl = 'ws://127.0.0.1:1234/ws/new';
    player.setAttribute('src', newUrl);
    player.attributeChangedCallback('src', oldUrl, newUrl);
    assert.equal(player.decodeIndicator.hidden, false);
});
