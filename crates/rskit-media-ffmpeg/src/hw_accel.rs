//! Hardware acceleration detection and configuration.

use serde::Deserialize;

/// Hardware acceleration mode for FFmpeg.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum HwAccel {
    /// Explicitly disable hardware acceleration (software-only decode).
    None,
    /// macOS VideoToolbox.
    VideoToolbox,
    /// NVIDIA CUDA/NVENC.
    Cuda,
    /// Intel Quick Sync Video.
    Qsv,
    /// VA-API (Linux).
    Vaapi,
    /// Vulkan.
    Vulkan,
    /// Direct3D 11 Video Acceleration.
    D3d11va,
    /// Let FFmpeg auto-detect the best available hardware acceleration.
    Auto,
}

impl HwAccel {
    /// Convert to the FFmpeg `-hwaccel` argument value.
    /// Returns `None` for [`HwAccel::None`] since no flag should be emitted.
    pub fn ffmpeg_arg(&self) -> Option<&str> {
        match self {
            Self::None => Option::None,
            Self::VideoToolbox => Some("videotoolbox"),
            Self::Cuda => Some("cuda"),
            Self::Qsv => Some("qsv"),
            Self::Vaapi => Some("vaapi"),
            Self::Vulkan => Some("vulkan"),
            Self::D3d11va => Some("d3d11va"),
            Self::Auto => Some("auto"),
        }
    }

    /// Whether this represents a hardware-accelerated mode (not software-only).
    pub fn is_hardware(&self) -> bool {
        !matches!(self, Self::None)
    }

    /// Detect available hardware acceleration methods by querying FFmpeg.
    pub async fn detect_available() -> Vec<HwAccel> {
        let output = tokio::process::Command::new("ffmpeg")
            .args(["-hide_banner", "-hwaccels"])
            .output()
            .await;

        let Ok(output) = output else {
            return Vec::new();
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut available = Vec::new();

        for line in stdout.lines() {
            let line = line.trim();
            match line {
                "videotoolbox" => available.push(HwAccel::VideoToolbox),
                "cuda" => available.push(HwAccel::Cuda),
                "qsv" => available.push(HwAccel::Qsv),
                "vaapi" => available.push(HwAccel::Vaapi),
                "vulkan" => available.push(HwAccel::Vulkan),
                "d3d11va" => available.push(HwAccel::D3d11va),
                _ => {}
            }
        }

        available
    }
}
