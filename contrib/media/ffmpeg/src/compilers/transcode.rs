//! Compiler for the `Transcode` operation (output configuration).

use rskit_errors::AppResult;
use rskit_media::output::{Bitrate, EncodingSpeed, OutputConfig, Quality, StreamingConfig};

use super::CompileContext;

pub(crate) fn compile_transcode(ctx: &mut CompileContext, config: &OutputConfig) -> AppResult<()> {
    if let Some(video) = &config.video {
        let encoder = ctx
            .registry
            .codec_info(&video.codec)
            .and_then(|info| info.ffmpeg_encoder.clone())
            .unwrap_or_else(|| video.codec.id().to_string());

        ctx.cmd.output_opts.extend(["-c:v".into(), encoder]);

        if let Some(quality) = &video.quality {
            let crf = match quality {
                Quality::Lossless => "0",
                Quality::UltraHigh => "14",
                Quality::High => "18",
                Quality::Medium => "23",
                Quality::Low => "28",
                Quality::VeryLow => "35",
                Quality::Custom(v) => {
                    ctx.cmd.output_opts.extend(["-crf".into(), v.to_string()]);
                    ""
                }
            };
            if !crf.is_empty() {
                ctx.cmd.output_opts.extend(["-crf".into(), crf.into()]);
            }
        }

        if let Some(bitrate) = &video.bitrate {
            match bitrate {
                Bitrate::Constant(br) => {
                    ctx.cmd.output_opts.extend(["-b:v".into(), br.to_string()]);
                }
                Bitrate::Variable(br) => {
                    ctx.cmd.output_opts.extend(["-b:v".into(), br.to_string()]);
                }
                Bitrate::Constrained { target, max } => {
                    ctx.cmd
                        .output_opts
                        .extend(["-b:v".into(), target.to_string()]);
                    ctx.cmd
                        .output_opts
                        .extend(["-maxrate".into(), max.to_string()]);
                }
            }
        }

        if let Some(speed) = &video.speed {
            let preset = match speed {
                EncodingSpeed::UltraFast => "ultrafast",
                EncodingSpeed::SuperFast => "superfast",
                EncodingSpeed::VeryFast => "veryfast",
                EncodingSpeed::Fast => "fast",
                EncodingSpeed::Medium => "medium",
                EncodingSpeed::Slow => "slow",
                EncodingSpeed::VerySlow => "veryslow",
            };
            ctx.cmd
                .output_opts
                .extend(["-preset".into(), preset.into()]);
        }

        if let Some(res) = &video.resolution {
            ctx.cmd
                .video_filters
                .push(format!("scale={}:{}", res.width, res.height));
        }

        if let Some(fps) = &video.frame_rate {
            ctx.cmd
                .output_opts
                .extend(["-r".into(), format!("{}/{}", fps.num, fps.den)]);
        }

        if let Some(profile) = &video.profile {
            ctx.cmd
                .output_opts
                .extend(["-profile:v".into(), profile.as_ffmpeg_arg().into()]);
        }

        if let Some(level) = &video.level {
            ctx.cmd
                .output_opts
                .extend(["-level".into(), level.to_string()]);
        }
    }

    if let Some(audio) = &config.audio {
        let encoder = ctx
            .registry
            .codec_info(&audio.codec)
            .and_then(|info| info.ffmpeg_encoder.clone())
            .unwrap_or_else(|| audio.codec.id().to_string());

        ctx.cmd.output_opts.extend(["-c:a".into(), encoder]);

        if let Some(sr) = &audio.sample_rate {
            ctx.cmd.output_opts.extend(["-ar".into(), sr.0.to_string()]);
        }

        if let Some(ch) = &audio.channels {
            ctx.cmd
                .output_opts
                .extend(["-ac".into(), ch.channel_count().to_string()]);
        }

        if let Some(bitrate) = &audio.bitrate {
            match bitrate {
                Bitrate::Constant(br) | Bitrate::Variable(br) => {
                    ctx.cmd.output_opts.extend(["-b:a".into(), br.to_string()]);
                }
                Bitrate::Constrained { target, .. } => {
                    ctx.cmd
                        .output_opts
                        .extend(["-b:a".into(), target.to_string()]);
                }
            }
        }
    }

    // Format extension for output
    if let Some(info) = ctx.registry.format_info(&config.format) {
        ctx.cmd
            .output_opts
            .extend(["-f".into(), info.extension.clone()]);
    }

    if config.strip_metadata {
        ctx.cmd
            .output_opts
            .extend(["-map_metadata".into(), "-1".into()]);
    }

    for (k, v) in &config.extra {
        ctx.cmd.output_opts.extend([format!("-{k}"), v.clone()]);
    }

    // Streaming output configuration
    if let Some(streaming) = &config.streaming {
        match streaming {
            StreamingConfig::Hls(hls) => {
                ctx.cmd.output_opts.extend(["-f".into(), "hls".into()]);
                ctx.cmd
                    .output_opts
                    .extend(["-hls_time".into(), hls.segment_duration.to_string()]);
                ctx.cmd
                    .output_opts
                    .extend(["-hls_list_size".into(), hls.playlist_size.to_string()]);
                match hls.playlist_type {
                    rskit_media::output::HlsPlaylistType::Vod => {
                        ctx.cmd
                            .output_opts
                            .extend(["-hls_playlist_type".into(), "vod".into()]);
                    }
                    rskit_media::output::HlsPlaylistType::Event => {
                        ctx.cmd
                            .output_opts
                            .extend(["-hls_playlist_type".into(), "event".into()]);
                    }
                }
                if let Some(seg_fn) = &hls.segment_filename {
                    ctx.cmd
                        .output_opts
                        .extend(["-hls_segment_filename".into(), seg_fn.clone()]);
                }
            }
            StreamingConfig::Dash(dash) => {
                ctx.cmd.output_opts.extend(["-f".into(), "dash".into()]);
                ctx.cmd
                    .output_opts
                    .extend(["-seg_duration".into(), dash.segment_duration.to_string()]);
                if dash.use_template {
                    ctx.cmd
                        .output_opts
                        .extend(["-use_template".into(), "1".into()]);
                }
                if dash.use_timeline {
                    ctx.cmd
                        .output_opts
                        .extend(["-use_timeline".into(), "1".into()]);
                }
            }
            StreamingConfig::Rtmp(rtmp) => {
                ctx.cmd.output_opts.extend(["-f".into(), "flv".into()]);
                ctx.cmd
                    .output_opts
                    .extend(["-rtmp_live".into(), "live".into()]);
                ctx.cmd.output_opts.push(rtmp.url.clone());
            }
        }
    }

    Ok(())
}
