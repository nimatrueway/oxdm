//! Minimal Matroska writer for recording the native RTSP pipeline.
//!
//! Just enough EBML to produce a file VLC/IINA/ffmpeg open: one video track
//! (`V_MPEG4/ISO/AVC` or `V_MPEGH/ISO/HEVC`, `CodecPrivate` = `avcC`/`hvcC`,
//! frames in their native length-prefixed form) and an optional
//! `A_PCM/INT/LIT` s16 audio track — which is why MKV and not MP4: the
//! G.711 audio most cameras send has no MP4 sample entry, but we already
//! decode it to PCM for the player.
//!
//! Clusters are buffered in memory and flushed on every video key frame (or
//! after 30 s, the `SimpleBlock` relative-timestamp range). `Segment` uses
//! the unknown-size marker; `Duration` is patched in place on `finish`. No
//! `Cues` — players seek local files fine without them.

use std::fs::File;
use std::io::{self, BufWriter, Seek, SeekFrom, Write};
use std::path::Path;

const ID_EBML: u32 = 0x1A45_DFA3;
const ID_EBML_VERSION: u32 = 0x4286;
const ID_EBML_READ_VERSION: u32 = 0x42F7;
const ID_EBML_MAX_ID_LENGTH: u32 = 0x42F2;
const ID_EBML_MAX_SIZE_LENGTH: u32 = 0x42F3;
const ID_DOC_TYPE: u32 = 0x4282;
const ID_DOC_TYPE_VERSION: u32 = 0x4287;
const ID_DOC_TYPE_READ_VERSION: u32 = 0x4285;
const ID_SEGMENT: u32 = 0x1853_8067;
const ID_INFO: u32 = 0x1549_A966;
const ID_TIMESTAMP_SCALE: u32 = 0x2AD7B1;
const ID_MUXING_APP: u32 = 0x4D80;
const ID_WRITING_APP: u32 = 0x5741;
const ID_DURATION: u32 = 0x4489;
const ID_TRACKS: u32 = 0x1654_AE6B;
const ID_TRACK_ENTRY: u32 = 0xAE;
const ID_TRACK_NUMBER: u32 = 0xD7;
const ID_TRACK_UID: u32 = 0x73C5;
const ID_TRACK_TYPE: u32 = 0x83;
const ID_FLAG_LACING: u32 = 0x9C;
const ID_CODEC_ID: u32 = 0x86;
const ID_CODEC_PRIVATE: u32 = 0x63A2;
const ID_VIDEO: u32 = 0xE0;
const ID_PIXEL_WIDTH: u32 = 0xB0;
const ID_PIXEL_HEIGHT: u32 = 0xBA;
const ID_AUDIO: u32 = 0xE1;
const ID_SAMPLING_FREQUENCY: u32 = 0xB5;
const ID_CHANNELS: u32 = 0x9F;
const ID_BIT_DEPTH: u32 = 0x6264;
const ID_CLUSTER: u32 = 0x1F43_B675;
const ID_CLUSTER_TIMESTAMP: u32 = 0xE7;
const ID_SIMPLE_BLOCK: u32 = 0xA3;

const TRACK_VIDEO: u64 = 1;
const TRACK_AUDIO: u64 = 2;
/// One tick = 1 ms.
const TIMESTAMP_SCALE_NS: u64 = 1_000_000;
/// `SimpleBlock` carries an i16 offset from its cluster; stay well inside.
const MAX_CLUSTER_MS: u64 = 30_000;
const UNKNOWN_SIZE: [u8; 8] = [0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];

pub struct VideoTrack<'a> {
    /// `avc1` or `hev1`/`hvc1` prefix decides the CodecID.
    pub rfc6381_codec: &'a str,
    pub config_record: &'a [u8],
    pub width: u32,
    pub height: u32,
}

pub struct AudioTrack {
    pub sample_rate: u32,
    pub channels: u16,
}

pub struct MkvWriter {
    out: BufWriter<File>,
    duration_pos: u64,
    cluster: Vec<u8>,
    cluster_ts_ms: u64,
    last_ts_ms: u64,
    has_audio: bool,
}

impl MkvWriter {
    pub fn create(
        path: &Path,
        video: &VideoTrack<'_>,
        audio: Option<&AudioTrack>,
    ) -> io::Result<Self> {
        let mut out = BufWriter::new(File::create(path)?);

        let mut header = Vec::new();
        uint(&mut header, ID_EBML_VERSION, 1);
        uint(&mut header, ID_EBML_READ_VERSION, 1);
        uint(&mut header, ID_EBML_MAX_ID_LENGTH, 4);
        uint(&mut header, ID_EBML_MAX_SIZE_LENGTH, 8);
        string(&mut header, ID_DOC_TYPE, "matroska");
        uint(&mut header, ID_DOC_TYPE_VERSION, 4);
        uint(&mut header, ID_DOC_TYPE_READ_VERSION, 2);
        let mut buf = Vec::new();
        element(&mut buf, ID_EBML, &header);

        write_id(&mut buf, ID_SEGMENT)?;
        buf.extend_from_slice(&UNKNOWN_SIZE);

        let app = format!("oxdm/{}", env!("CARGO_PKG_VERSION"));
        let mut info = Vec::new();
        uint(&mut info, ID_TIMESTAMP_SCALE, TIMESTAMP_SCALE_NS);
        string(&mut info, ID_MUXING_APP, &app);
        string(&mut info, ID_WRITING_APP, &app);
        // Duration payload offset inside `info`; resolved to a file offset below.
        let duration_rel = info.len() + id_len(ID_DURATION) + 1;
        float(&mut info, ID_DURATION, 0.0);
        let info_start = buf.len() + id_len(ID_INFO) + size_len(info.len() as u64);
        element(&mut buf, ID_INFO, &info);
        let duration_pos = (info_start + duration_rel) as u64;

        let mut tracks = Vec::new();
        let mut v = Vec::new();
        uint(&mut v, ID_TRACK_NUMBER, TRACK_VIDEO);
        uint(&mut v, ID_TRACK_UID, TRACK_VIDEO);
        uint(&mut v, ID_TRACK_TYPE, 1);
        uint(&mut v, ID_FLAG_LACING, 0);
        let codec_id = if video.rfc6381_codec.starts_with("avc1") {
            "V_MPEG4/ISO/AVC"
        } else {
            "V_MPEGH/ISO/HEVC"
        };
        string(&mut v, ID_CODEC_ID, codec_id);
        binary(&mut v, ID_CODEC_PRIVATE, video.config_record);
        let mut vid = Vec::new();
        uint(&mut vid, ID_PIXEL_WIDTH, u64::from(video.width));
        uint(&mut vid, ID_PIXEL_HEIGHT, u64::from(video.height));
        element(&mut v, ID_VIDEO, &vid);
        element(&mut tracks, ID_TRACK_ENTRY, &v);

        if let Some(a) = audio {
            let mut t = Vec::new();
            uint(&mut t, ID_TRACK_NUMBER, TRACK_AUDIO);
            uint(&mut t, ID_TRACK_UID, TRACK_AUDIO);
            uint(&mut t, ID_TRACK_TYPE, 2);
            uint(&mut t, ID_FLAG_LACING, 0);
            string(&mut t, ID_CODEC_ID, "A_PCM/INT/LIT");
            let mut aud = Vec::new();
            float(&mut aud, ID_SAMPLING_FREQUENCY, f64::from(a.sample_rate));
            uint(&mut aud, ID_CHANNELS, u64::from(a.channels));
            uint(&mut aud, ID_BIT_DEPTH, 16);
            element(&mut t, ID_AUDIO, &aud);
            element(&mut tracks, ID_TRACK_ENTRY, &t);
        }
        element(&mut buf, ID_TRACKS, &tracks);

        out.write_all(&buf)?;
        Ok(Self {
            out,
            duration_pos,
            cluster: Vec::new(),
            cluster_ts_ms: 0,
            last_ts_ms: 0,
            has_audio: audio.is_some(),
        })
    }

    /// `ts_ms` is relative to the recording start and must be non-decreasing
    /// per track. `key` opens a new cluster.
    pub fn write_video(&mut self, ts_ms: u64, key: bool, data: &[u8]) -> io::Result<()> {
        if key || self.cluster.is_empty() || ts_ms >= self.cluster_ts_ms + MAX_CLUSTER_MS {
            self.begin_cluster(ts_ms)?;
        }
        self.block(TRACK_VIDEO, ts_ms, key, data)
    }

    pub fn write_audio(&mut self, ts_ms: u64, pcm: &[u8]) -> io::Result<()> {
        if !self.has_audio {
            return Ok(());
        }
        // Audio ahead of the first video key frame has no cluster yet; a
        // cluster that would overflow the i16 offset gets rolled too.
        if self.cluster.is_empty() || ts_ms >= self.cluster_ts_ms + MAX_CLUSTER_MS {
            self.begin_cluster(ts_ms)?;
        }
        self.block(TRACK_AUDIO, ts_ms, true, pcm)
    }

    pub fn finish(mut self) -> io::Result<()> {
        self.flush_cluster()?;
        self.out.flush()?;
        self.out.seek(SeekFrom::Start(self.duration_pos))?;
        self.out
            .write_all(&(self.last_ts_ms as f64).to_be_bytes())?;
        self.out.flush()
    }

    fn begin_cluster(&mut self, ts_ms: u64) -> io::Result<()> {
        self.flush_cluster()?;
        self.cluster_ts_ms = ts_ms;
        uint(&mut self.cluster, ID_CLUSTER_TIMESTAMP, ts_ms);
        Ok(())
    }

    fn flush_cluster(&mut self) -> io::Result<()> {
        if self.cluster.is_empty() {
            return Ok(());
        }
        write_id(&mut self.out, ID_CLUSTER)?;
        write_size(&mut self.out, self.cluster.len() as u64)?;
        self.out.write_all(&self.cluster)?;
        self.cluster.clear();
        Ok(())
    }

    fn block(&mut self, track: u64, ts_ms: u64, key: bool, data: &[u8]) -> io::Result<()> {
        // Timestamps within a cluster are monotonic by construction, but a
        // late audio frame can land under the cluster start; clamp it.
        let rel = ts_ms
            .saturating_sub(self.cluster_ts_ms)
            .min(i16::MAX as u64) as u16;
        let mut head = Vec::with_capacity(4);
        write_size(&mut head, track)?;
        head.extend_from_slice(&rel.to_be_bytes());
        head.push(if key { 0x80 } else { 0x00 });
        write_id(&mut self.cluster, ID_SIMPLE_BLOCK)?;
        write_size(&mut self.cluster, (head.len() + data.len()) as u64)?;
        self.cluster.extend_from_slice(&head);
        self.cluster.extend_from_slice(data);
        self.last_ts_ms = self.last_ts_ms.max(ts_ms);
        Ok(())
    }
}

// ── EBML primitives ──────────────────────────────────────────────────────────

fn id_len(id: u32) -> usize {
    4 - (id.leading_zeros() / 8) as usize
}

fn write_id<W: Write>(w: &mut W, id: u32) -> io::Result<()> {
    let n = id_len(id);
    w.write_all(&id.to_be_bytes()[4 - n..])
}

fn size_len(size: u64) -> usize {
    // A vint of n bytes holds 7n-1 usable bits (all-ones is reserved).
    (1..=8).find(|n| size < (1u64 << (7 * n)) - 1).unwrap_or(8)
}

fn write_size<W: Write>(w: &mut W, size: u64) -> io::Result<()> {
    let n = size_len(size);
    let mut bytes = size.to_be_bytes();
    bytes[8 - n] |= 0x80 >> (n - 1);
    w.write_all(&bytes[8 - n..])
}

fn element(buf: &mut Vec<u8>, id: u32, payload: &[u8]) {
    let _ = write_id(buf, id);
    let _ = write_size(buf, payload.len() as u64);
    buf.extend_from_slice(payload);
}

fn uint(buf: &mut Vec<u8>, id: u32, v: u64) {
    let bytes = v.to_be_bytes();
    let skip = (v.leading_zeros() / 8).min(7) as usize;
    element(buf, id, &bytes[skip..]);
}

fn float(buf: &mut Vec<u8>, id: u32, v: f64) {
    element(buf, id, &v.to_be_bytes());
}

fn string(buf: &mut Vec<u8>, id: u32, s: &str) {
    element(buf, id, s.as_bytes());
}

fn binary(buf: &mut Vec<u8>, id: u32, b: &[u8]) {
    element(buf, id, b);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vint_sizes_use_minimal_length_with_marker_bit() {
        let mut b = Vec::new();
        write_size(&mut b, 5).unwrap();
        assert_eq!(b, [0x85]);
        b.clear();
        write_size(&mut b, 127).unwrap(); // 0x7F is reserved in 1 byte → 2 bytes
        assert_eq!(b, [0x40, 0x7F]);
        b.clear();
        write_size(&mut b, 300).unwrap();
        assert_eq!(b, [0x41, 0x2C]);
    }

    #[test]
    fn ids_are_written_verbatim() {
        let mut b = Vec::new();
        write_id(&mut b, ID_SEGMENT).unwrap();
        write_id(&mut b, ID_SIMPLE_BLOCK).unwrap();
        assert_eq!(b, [0x18, 0x53, 0x80, 0x67, 0xA3]);
    }

    #[test]
    fn uint_is_minimal_but_never_empty() {
        let mut b = Vec::new();
        uint(&mut b, ID_TRACK_NUMBER, 0);
        assert_eq!(b, [0xD7, 0x81, 0x00]);
        b.clear();
        uint(&mut b, ID_TRACK_NUMBER, 0x1234);
        assert_eq!(b, [0xD7, 0x82, 0x12, 0x34]);
    }

    #[test]
    fn duration_is_patched_on_finish() {
        let dir = std::env::temp_dir().join(format!("oxdm-mkv-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.mkv");
        let video = VideoTrack {
            rfc6381_codec: "avc1.640032",
            config_record: &[1, 0x64, 0, 0x32, 0xFF, 0xE0, 0],
            width: 16,
            height: 16,
        };
        let audio = AudioTrack {
            sample_rate: 8000,
            channels: 1,
        };
        let mut w = MkvWriter::create(&path, &video, Some(&audio)).unwrap();
        let pos = w.duration_pos as usize;
        w.write_video(0, true, &[0, 0, 0, 1, 0x65]).unwrap();
        w.write_audio(10, &[0, 0, 0, 0]).unwrap();
        w.write_video(66, false, &[0, 0, 0, 1, 0x41]).unwrap();
        w.write_video(2000, true, &[0, 0, 0, 1, 0x65]).unwrap();
        w.finish().unwrap();
        let bytes = std::fs::read(&path).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(&bytes[..4], &[0x1A, 0x45, 0xDF, 0xA3]);
        assert_eq!(
            &bytes[pos - 3..pos],
            &[0x44, 0x89, 0x88],
            "Duration id+size"
        );
        let dur = f64::from_be_bytes(bytes[pos..pos + 8].try_into().unwrap());
        assert_eq!(dur, 2000.0);
        // Two clusters (key at 0, key at 2000).
        assert_eq!(
            bytes
                .windows(4)
                .filter(|w| *w == [0x1F, 0x43, 0xB6, 0x75])
                .count(),
            2
        );
    }
}
