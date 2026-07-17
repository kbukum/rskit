use rskit_errors::ErrorCode;
use rskit_media::{ChannelLayout, MediaType};
use rskit_storage::FileSource;

use super::*;
use crate::probe::{channel_layout, metadata_for_wav, read_bounded, unsupported};
use crate::wav::{WavReader, WavSpec};

#[test]
fn config_builders_and_channel_layouts_are_deterministic() {
    let config = Config::default()
        .with_max_probe_bytes(128)
        .with_metadata_waveform_bins(4);

    assert_eq!(config.max_probe_bytes, 128);
    assert_eq!(config.metadata_waveform_bins, 4);
    assert_eq!(channel_layout(1), ChannelLayout::Mono);
    assert_eq!(channel_layout(2), ChannelLayout::Stereo);
    assert_eq!(channel_layout(6), ChannelLayout::Surround51);
    assert_eq!(channel_layout(8), ChannelLayout::Surround71);
    assert_eq!(channel_layout(3), ChannelLayout::Custom(3));
}

#[test]
fn unsupported_returns_invalid_input() {
    let err = unsupported::<()>("not supported").unwrap_err();

    assert_eq!(err.code(), ErrorCode::InvalidInput);
    assert!(err.message().contains("not supported"));
}

#[tokio::test]
async fn read_bounded_rejects_sources_over_limit() {
    let source = FileSource::Bytes(bytes::Bytes::from_static(b"abcdef"));

    let err = read_bounded(&source, 3).await.unwrap_err();

    assert_eq!(err.code(), ErrorCode::InvalidInput);
    assert!(err.message().contains("exceeds probe limit"));
}

#[test]
fn metadata_for_wav_handles_custom_channels_and_empty_waveform() {
    let wav = WavReader {
        spec: WavSpec {
            channels: 3,
            sample_rate: 48_000,
            bits_per_sample: 16,
        },
        samples: vec![0.0; 9],
    };

    let metadata = metadata_for_wav(&wav, 0);

    assert_eq!(metadata.media_type, MediaType::Audio);
    assert_eq!(metadata.bitrate, Some(48_000 * 3 * 16));
    assert_eq!(
        metadata.tags.get("audio.waveform_bins").map(String::as_str),
        Some("0")
    );
    assert!(!metadata.tags.contains_key("audio.waveform_peak"));
    let track = metadata.tracks.first().unwrap();
    assert_eq!(
        track.audio.as_ref().map(|audio| audio.channels),
        Some(ChannelLayout::Custom(3))
    );
}
