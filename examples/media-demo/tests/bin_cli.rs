use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn unique_path(name: &str) -> PathBuf {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/{name}-{}-{}.wav",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    ))
}

fn silent_wav() -> Vec<u8> {
    let sample_count = 800_u32;
    let data_len = sample_count * 2;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&8000_u32.to_le_bytes());
    bytes.extend_from_slice(&16000_u32.to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    bytes.resize(bytes.len() + usize::try_from(data_len).unwrap(), 0);
    bytes
}

#[test]
fn audio_analysis_binary_formats_generated_wav() {
    let path = unique_path("audio-analysis-bin");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, silent_wav()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_audio_analysis"))
        .arg(&path)
        .output()
        .unwrap();
    let _ = fs::remove_file(&path);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("=== WAV Info:"));
    assert!(stdout.contains("Channels"));
}

#[test]
fn ffmpeg_binaries_return_errors_for_missing_inputs() {
    for (exe, args) in [
        (env!("CARGO_BIN_EXE_probe"), vec!["missing-input.mp4"]),
        (
            env!("CARGO_BIN_EXE_thumbnail"),
            vec!["missing-input.mp4", "missing-thumb.jpg", "1.0"],
        ),
        (
            env!("CARGO_BIN_EXE_transcode"),
            vec!["missing-input.mp4", "missing-output.mp4"],
        ),
    ] {
        let output = Command::new(exe).args(args).output().unwrap();
        assert!(
            !output.status.success(),
            "{exe} should fail for missing input"
        );
    }
}
