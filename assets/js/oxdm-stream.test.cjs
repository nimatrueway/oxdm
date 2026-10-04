const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');

class Element {
    constructor() {
        this.listeners = new Map();
        this.classList = { toggle() {} };
    }

    append() {}
    appendChild(child) { return child; }
    replaceWith() {}
    getContext() { return {}; }
    setAttribute() {}
    addEventListener(name, handler) { this.listeners.set(name, handler); }
    getAttribute(name) {
        if (name === 'data-labels') {
            return JSON.stringify({
                waiting: { label: 'Waiting', title: 'Waiting' },
                paused: { label: 'Paused', title: 'Paused' },
                'software-mjpeg': { label: 'Software MJPEG', title: 'Software MJPEG' },
                dismiss: 'Hide decoder badge',
            });
        }
        return 'ws://127.0.0.1:1234/ws/test';
    }
}

function lastAudioPreference(sent) {
    return sent.filter(message => message.type === 'audio').at(-1);
}

function playerFor(audio) {
    let Player;
    const sent = [];
    const scheduled = [];
    class AudioContext {
        constructor() {
            this.state = 'running';
            this.currentTime = 0;
        }
        createBuffer(channels, frames, sampleRate) {
            return {
                duration: frames / sampleRate,
                getChannelData: () => new Float32Array(frames),
            };
        }
        createBufferSource() {
            return { connect() {}, start: time => scheduled.push(time) };
        }
    }
    const sandbox = {
        HTMLElement: Element,
        document: { createElement: () => new Element(), addEventListener() {} },
        customElements: { define: (_, constructor) => { Player = constructor; } },
        window: { AudioContext },
        WebSocket: { OPEN: 1 },
        performance: { now: () => 0 },
        console: { warn() {} },
    };
    vm.runInNewContext(
        fs.readFileSync(path.join(__dirname, 'oxdm-stream.js'), 'utf8'),
        sandbox,
    );
    const player = new Player();
    player.build();
    player.ws = { readyState: 1, send: message => sent.push(JSON.parse(message)) };
    player.onInit({ type: 'init', audio });
    return { player, sent, scheduled };
}

test('MJPEG video fallback keeps advertised WebSocket audio available and muted initially', () => {
    const { player, sent, scheduled } = playerFor({ channels: 1, sample_rate: 8000 });
    assert.equal(player.img.src, 'http://127.0.0.1:1234/mjpeg/test');
    assert.equal(player.muted, true);
    assert.equal(player.muteBtn.disabled, false);
    assert.equal(player.muteBtn.title, 'Unmute');
    assert.deepEqual(lastAudioPreference(sent), { type: 'audio', enabled: false });

    player.muteBtn.listeners.get('click')();
    assert.equal(player.muted, false);
    assert.equal(player.muteBtn.title, 'Mute');
    assert.deepEqual(lastAudioPreference(sent), { type: 'audio', enabled: true });
    const frame = new ArrayBuffer(14);
    new DataView(frame).setUint8(0, 1);
    player.onFrame(frame);
    assert.equal(scheduled.length, 1);

    player.muteBtn.listeners.get('click')();
    assert.equal(player.muted, true);
    assert.deepEqual(lastAudioPreference(sent), { type: 'audio', enabled: false });
    player.onFrame(frame);
    assert.equal(scheduled.length, 1);
});

test('streams without audio keep the mute control disabled', () => {
    const { player, sent } = playerFor(null);
    assert.equal(player.muteBtn.disabled, true);
    assert.deepEqual(lastAudioPreference(sent), { type: 'audio', enabled: false });
});

test('a new init on the MJPEG fallback refreshes audio availability and preference', () => {
    const { player, sent } = playerFor({ channels: 1, sample_rate: 8000 });
    player.muteBtn.listeners.get('click')();
    player.onInit({ type: 'init', audio: null });
    assert.equal(player.muteBtn.disabled, true);
    assert.deepEqual(lastAudioPreference(sent), { type: 'audio', enabled: false });
    player.onInit({ type: 'init', audio: { channels: 1, sample_rate: 8000 } });
    assert.equal(player.muteBtn.disabled, false);
    assert.deepEqual(lastAudioPreference(sent), { type: 'audio', enabled: true });
});
