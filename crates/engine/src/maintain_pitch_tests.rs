//! Clip ▸ Speed/Duration ▸ Maintain Audio Pitch: a 440 Hz tone at 125 % keeps 440 Hz in the exported
//! sound (and takes 1/1.25 of the time); without the option it plays at 550 Hz, as before.

use filmcraft_project::ClipId;
use serde_json::json;

use crate::Session;

const SR: u32 = 48_000;

/// A session whose active 24 fps sequence holds one clip of a 2.5 s mono 440 Hz WAV on A1.
fn tone_clip(dir: &std::path::Path) -> (Session, ClipId) {
    let wav = dir.join("tone.wav");
    let samples: Vec<f32> = (0..SR as usize * 5 / 2).map(|i| (std::f64::consts::TAU * 440.0 * i as f64 / f64::from(SR)).sin() as f32 * 0.5).collect();
    std::fs::write(&wav, filmcraft_media::wav::write_wav16(&samples, 1, SR)).unwrap();
    let mut s = Session::default();
    let r = s.execute("file.import", json!({"paths": [wav.to_string_lossy()]})).unwrap();
    let item = r["items"][0].as_u64().unwrap();
    s.execute("file.newSequence", json!({"name": "Pitch", "fromItems": [item], "fps": 24, "width": 160, "height": 90})).unwrap();
    let c = s.active_sequence().unwrap().audio_tracks[0].items[0].id;
    (s, c)
}

/// The left channel of a 16-bit PCM WAV file.
fn left_channel(path: &std::path::Path) -> Vec<f32> {
    let b = std::fs::read(path).unwrap();
    let channels = u16::from_le_bytes([b[22], b[23]]) as usize;
    let mut at = 12;
    while &b[at..at + 4] != b"data" {
        at += 8 + u32::from_le_bytes(b[at + 4..at + 8].try_into().unwrap()) as usize;
    }
    let data = &b[at + 8..];
    data.chunks_exact(2 * channels).map(|f| f32::from(i16::from_le_bytes([f[0], f[1]])) / 32768.0).collect()
}

/// The strongest frequency between 200 and 800 Hz, in 0.25 Hz steps.
fn dominant_hz(x: &[f32]) -> f64 {
    let level = |hz: f64| {
        let (mut re, mut im) = (0.0, 0.0);
        for (i, v) in x.iter().enumerate() {
            let ph = std::f64::consts::TAU * hz * i as f64 / f64::from(SR);
            re += f64::from(*v) * ph.cos();
            im += f64::from(*v) * ph.sin();
        }
        re.hypot(im)
    };
    (800..=3200).map(|q| f64::from(q) / 4.0).map(|hz| (hz, level(hz))).fold((0.0, 0.0), |a, b| if b.1 > a.1 { b } else { a }).0
}

fn export(s: &mut Session, path: &std::path::Path) -> Vec<f32> {
    s.execute("export.queue.add", json!({"preset": "Waveform Audio 48 kHz 16-bit", "path": path.to_string_lossy()})).unwrap();
    s.execute("export.queue.start", json!({"wait": true})).unwrap();
    left_channel(path)
}

#[test]
fn maintain_audio_pitch_keeps_the_tone_at_its_pitch_in_the_export() {
    let dir = crate::media_test_util::tmp_dir("maintain-pitch");
    for (maintain, want) in [(true, 440.0), (false, 550.0)] {
        let (mut s, c) = tone_clip(&dir);
        s.state.selection = vec![c];
        s.execute("clip.speedDuration", json!({"speed": 125, "maintainPitch": maintain})).unwrap();
        let clip = s.active_sequence().unwrap().find_item(c).unwrap().1.clone();
        assert_eq!(clip.maintain_pitch, maintain);
        let out = export(&mut s, &dir.join(format!("out-{maintain}.wav")));
        // 2.5 s of source at 125 % takes 2 s
        assert_eq!(out.len(), SR as usize * 2, "maintain {maintain}");
        // a second from the middle, clear of the pitch shifter's start
        let hz = dominant_hz(&out[SR as usize / 2..SR as usize * 3 / 2]);
        assert!((hz - want).abs() <= want * 0.02, "maintain {maintain}: {hz} Hz, want {want} Hz");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn maintain_audio_pitch_is_stored_undone_and_saved() {
    let dir = crate::media_test_util::tmp_dir("maintain-pitch-store");
    let (mut s, c) = tone_clip(&dir);
    s.state.selection = vec![c];
    let clip = |s: &Session| s.active_sequence().unwrap().find_item(c).unwrap().1.clone();
    assert!(!clip(&s).maintain_pitch, "off by default, as in Premiere");
    s.execute("clip.speedDuration", json!({"speed": 80, "maintainPitch": true})).unwrap();
    assert!(clip(&s).maintain_pitch);
    // absent leaves it as it is
    s.execute("clip.speedDuration", json!({"speed": 90})).unwrap();
    assert!(clip(&s).maintain_pitch);
    // a non-boolean value is not taken for one
    s.execute("clip.speedDuration", json!({"speed": 90, "maintainPitch": "no"})).unwrap();
    assert!(clip(&s).maintain_pitch);
    // survives a save / load
    let bytes = filmcraft_format::encode(&s.project, true);
    let back = filmcraft_format::decode(&bytes).unwrap().project;
    let seq = s.state.active_sequence.unwrap();
    assert_eq!(back.sequence(seq).unwrap().find_item(c).unwrap().1, &clip(&s));
    // undo returns to the clip without it
    s.execute("edit.undo", json!({})).unwrap();
    s.execute("edit.undo", json!({})).unwrap();
    s.execute("edit.undo", json!({})).unwrap();
    assert!(!clip(&s).maintain_pitch);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Hostile speeds with Maintain Audio Pitch on must not panic: the correction is limited, and a
/// clip at any speed the edit accepts still renders.
#[test]
fn extreme_speeds_with_maintain_pitch_render_without_panicking() {
    let dir = crate::media_test_util::tmp_dir("maintain-pitch-extreme");
    for speed in [0.01, 1.0, 99.0, 1000.0, 20_000.0, -50.0, f64::NAN] {
        let (mut s, c) = tone_clip(&dir);
        s.state.selection = vec![c];
        let _ = s.execute("clip.speedDuration", json!({"speed": speed, "maintainPitch": true}));
        let provider = s.media.provider(s.project.clone(), s.services.clone());
        let seq = s.active_sequence().unwrap();
        let mix = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| filmcraft_render::audio::mix_sequence(&s.project, seq, 0, 4800, &provider)));
        let mix = mix.unwrap_or_else(|_| panic!("speed {speed} panicked"));
        assert!(mix.channels.iter().flatten().all(|v| v.is_finite()), "speed {speed}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
