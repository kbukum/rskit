//! Compiler for the generic `Filter` operation.

use rskit_errors::AppResult;
use rskit_media::filter::{Filter, FilterTarget};

use super::CompileContext;
use crate::filter_map;

pub(crate) fn compile_filter(ctx: &mut CompileContext, filter: &Filter) -> AppResult<()> {
    let ff_filter = filter_map::to_ffmpeg_filter(filter);
    match filter.target {
        FilterTarget::Video => ctx.cmd.video_filters.push(ff_filter),
        FilterTarget::Audio => ctx.cmd.audio_filters.push(ff_filter),
    }
    Ok(())
}
