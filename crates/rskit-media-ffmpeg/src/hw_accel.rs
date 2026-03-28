//! Hardware acceleration detection and configuration.

use serde::Deserialize;

/// Hardware acceleration mode for FFmpeg.
#[derive(Debug, Clone, Copy, Deserialize)]
pub enum HwAccel {
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
    pub fn ffmpeg_arg(&self) -> &str {
        match self {
            Self::VideoToolbox => "videotoolbox",
            Self::Cuda => "cuda",
            Self::Qsv => "qsv",
            Self::Vaapi => "vaapi",
            Self::Vulkan => "vulkan",
            Self::D3d11va => "d3d11va",
            Self::Auto => "auto",
        }
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
