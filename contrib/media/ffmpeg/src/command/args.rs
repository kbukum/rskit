use std::ffi::OsString;

use rskit_storage::FileSource;

use super::FfmpegCommand;

impl FfmpegCommand {
    /// Build the final FFmpeg CLI argument list (excluding the output path).
    pub(crate) fn to_args(&self) -> Vec<String> {
        self.to_os_args()
            .into_iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    /// Build the final FFmpeg CLI argument list preserving OS-native path arguments.
    pub(crate) fn to_os_args(&self) -> Vec<OsString> {
        let mut args = Vec::new();

        args.extend(self.global_opts.iter().map(OsString::from));

        for input in &self.inputs {
            if let Some(seek) = &input.seek_to {
                args.push(OsString::from("-ss"));
                args.push(OsString::from(seek.to_ffmpeg_time()));
            }
            if let Some(dur) = &input.duration {
                args.push(OsString::from("-t"));
                args.push(OsString::from(format!("{:.3}", dur.as_secs_f64())));
            }
            args.push(OsString::from("-i"));
            match &input.source {
                FileSource::Path(path) => args.push(path.as_os_str().to_os_string()),
                FileSource::Temp(temp) => args.push(temp.path().as_os_str().to_os_string()),
                _ => args.push(OsString::from("pipe:0")),
            }
        }

        if let Some(complex) = &self.complex_filter {
            args.push(OsString::from("-filter_complex"));
            args.push(OsString::from(complex));
        } else {
            if !self.video_filters.is_empty() {
                args.push(OsString::from("-vf"));
                args.push(OsString::from(self.video_filters.join(",")));
            }
            if !self.audio_filters.is_empty() {
                args.push(OsString::from("-af"));
                args.push(OsString::from(self.audio_filters.join(",")));
            }
        }

        args.extend(self.output_opts.iter().map(OsString::from));
        args
    }
}
