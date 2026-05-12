//! Compilers for track selection: SelectTracks, SelectTracksByKind.

use rskit_errors::AppResult;
use rskit_media::TrackKind;

use super::CompileContext;

pub(crate) fn compile_select_tracks(ctx: &mut CompileContext, indices: &[usize]) -> AppResult<()> {
    for idx in indices {
        ctx.cmd
            .output_opts
            .extend(["-map".into(), format!("0:{idx}")]);
    }
    Ok(())
}

pub(crate) fn compile_select_tracks_by_kind(
    ctx: &mut CompileContext,
    kinds: &[TrackKind],
) -> AppResult<()> {
    for kind in kinds {
        let stream_type = match kind {
            TrackKind::Video => "v",
            TrackKind::Audio => "a",
            TrackKind::Subtitle => "s",
            _ => continue,
        };
        ctx.cmd
            .output_opts
            .extend(["-map".into(), format!("0:{stream_type}")]);
    }
    Ok(())
}
