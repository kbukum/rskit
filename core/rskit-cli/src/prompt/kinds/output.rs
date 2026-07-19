use crate::prompt::render::Style;
use crate::prompt::terminal::Terminal;

/// Write the inline answer marker (`» [hint]: `, ASCII-safe via [`Glyphs`](crate::theme::Glyphs))
/// and flush, for line prompts.
pub(crate) fn write_answer(
    terminal: &mut (impl Terminal + ?Sized),
    style: Style,
    hint: Option<&str>,
) -> rskit_errors::AppResult<()> {
    let marker = style.glyphs().answer();
    let text = hint.map_or_else(
        || format!("  {marker} "),
        |hint| format!("  {marker} {hint}: "),
    );
    terminal.write(&text)?;
    terminal.flush()
}

/// Write a dimmed warning notice line beneath a line prompt.
pub(crate) fn notice(
    terminal: &mut (impl Terminal + ?Sized),
    style: Style,
    text: &str,
) -> rskit_errors::AppResult<()> {
    let styled = style.palette().warn(text).into_owned();
    terminal.write_line(&format!("  {styled}"))
}
