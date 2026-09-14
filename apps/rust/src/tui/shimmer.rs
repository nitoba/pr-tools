//! Helpers de progresso e integração com o efeito shimmer do `tui-shimmer`.

use ratatui::{
    style::Style,
    text::{Line, Span},
};
use tui_shimmer::shimmer_spans_with_style_at_phase;

use super::{ascii_only, colors_enabled, theme};

const TICKS_PER_SWEEP: u8 = 60;

/// `usize` → `u16` saturando (ex.: máx. de scroll).
#[must_use]
pub fn u16_from_usize_saturated(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

/// `i32` clampado `0..=max` → `u16` (ex.: scroll `0..5000`).
#[must_use]
pub fn u16_from_i32_clamped(value: i32, max: u16) -> u16 {
    u16::try_from(value.clamp(0, i32::from(max))).unwrap_or(u16::MAX)
}

/// `u16` → `i16` saturando (ex.: altura visível < 100 p/ `scroll_by`).
#[must_use]
pub fn i16_from_u16_saturated(value: u16) -> i16 {
    i16::try_from(value).unwrap_or(i16::MAX)
}

/// `usize` → `f64` sem perda para as contagens usadas pela TUI.
#[must_use]
pub fn f64_from_usize(value: usize) -> f64 {
    f64::from(u32::try_from(value).unwrap_or(u32::MAX))
}

/// Células preenchidas da barra (`ratio` `0..1`, `width` células).
#[must_use]
pub fn filled_cells(ratio: f64, width: usize) -> usize {
    let width_f64 = f64_from_usize(width);
    let filled = (ratio.clamp(0.0, 1.0) * width_f64).clamp(0.0, width_f64);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let filled = filled as usize;
    filled
}

/// Percentual `0..100` a partir de `ratio` `0..1`.
#[must_use]
pub fn percent_u16(ratio: f64) -> u16 {
    let percent = (ratio.clamp(0.0, 1.0) * 100.0).clamp(0.0, 100.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let percent = percent as u16;
    percent
}

fn phase(tick: u64) -> f32 {
    let tick = u8::try_from(tick % u64::from(TICKS_PER_SWEEP)).unwrap_or_default();
    f32::from(tick) / f32::from(TICKS_PER_SWEEP)
}

/// Renderiza texto com o shimmer do pacote, sincronizado ao tick da tela.
#[must_use]
pub fn shimmer_text(text: &str, style: Style, tick: u64) -> Line<'static> {
    if text.is_empty() || !colors_enabled() || ascii_only() {
        return Line::from(Span::styled(text.to_owned(), style));
    }
    Line::from(shimmer_spans_with_style_at_phase(text, style, phase(tick)))
}

/// Renderiza a barra mantendo a parte vazia estável e animando a parte preenchida.
#[must_use]
pub fn shimmer_bar(ratio: f64, width: usize, tick: u64, active: bool) -> Line<'static> {
    let width = width.max(8);
    let filled = filled_cells(ratio, width);
    let empty = width.saturating_sub(filled);
    if !active || filled == 0 || !colors_enabled() || ascii_only() {
        return Line::from(vec![
            Span::styled("█".repeat(filled), theme().accent),
            Span::styled("░".repeat(empty), theme().muted),
        ]);
    }

    let mut spans =
        shimmer_spans_with_style_at_phase(&"█".repeat(filled), theme().accent, phase(tick));
    if empty > 0 {
        spans.push(Span::styled("░".repeat(empty), theme().muted));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shimmer_should_span_full_text() {
        let line = shimmer_text("abc", theme().muted, 5);
        assert_eq!(
            line.spans
                .iter()
                .map(|span| span.content.chars().count())
                .sum::<usize>(),
            3
        );
    }

    #[test]
    fn bar_should_have_exact_width() {
        let line = shimmer_bar(0.5, 20, 3, true);
        assert_eq!(
            line.spans
                .iter()
                .map(|span| span.content.chars().count())
                .sum::<usize>(),
            20
        );
    }
}
