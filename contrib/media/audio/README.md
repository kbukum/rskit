# rskit-media-audio

Pure Rust audio processing backend for rskit media.

`rskit-media-audio` provides WAV probing and lightweight analysis without an FFmpeg dependency. It integrates with `rskit-media` through the media registry and is intended for bounded, in-process audio inspection tasks.

## Features

- WAV metadata probing.
- Waveform summary generation.
- Peak/RMS loudness measurement.
- Silence detection.
- Configurable probe byte limits.

For encoding, format conversion, and complex filter graphs, use the FFmpeg adapter instead.
