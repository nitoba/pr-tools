//! Efeito shimmer — faixa de brilho que varre texto/barras de progresso.
//!
//! Sem dependência de terminal verdadeiro: o shimmer é uma onda triangular
//! sobre os índices (`tick`), então cada frame (~30fps) move o brilho.
//! Usado no rótulo de fase e na barra de progresso enquanto gera.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use super::{colors_enabled, theme};

/// Largura da faixa brilhante (em células).
const BAND: i64 = 8;

/// Conversões saturantes da TUI — centraliza os casts pedantic.
///
/// Todos os valores aqui são provadamente limitados em uso real:
/// scroll `0..5000`, percentual `0..100`, larguras vindas de `Rect` (`u16`),
/// ticks que crescem a ~30fps. A saturação via `try_from().unwrap_or(MAX)`
/// nunca muda pixel em uso real; só evita wrap/truncate em 32-bit ou
/// overflow. Nenhum helper usa `unwrap`/`expect`.
/// `usize` → `u16` saturando em `u16::MAX` (ex.: máx. de scroll).
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

/// `usize` → `f64` sem perda pedantic (via `u32`, exato em `f64`).
///
/// Larguras/contagens reais (< 65535) nunca saturam; só listas
/// absurdas (> 4Bi itens) saturam em `u32::MAX`.
#[must_use]
pub fn f64_from_usize(value: usize) -> f64 {
    f64::from(u32::try_from(value).unwrap_or(u32::MAX))
}

/// `i64` → `f64` sem perda pedantic (via `i32`, exato em `f64`).
///
/// Índices do shimmer (`±centenas`) nunca saturam.
#[must_use]
pub fn f64_from_i64(value: i64) -> f64 {
    if let Ok(v) = i32::try_from(value) {
        f64::from(v)
    } else if value < 0 {
        f64::from(i32::MIN)
    } else {
        f64::from(i32::MAX)
    }
}

/// `f64` `0..255` → `u8` truncando (igual ao `as u8` original).
///
/// O valor já vem de interpolação `0..255`; o `clamp` só garante
/// o contrato p/ o cast.
#[must_use]
pub fn u8_from_f64_clamped(value: f64) -> u8 {
    let v = value.clamp(0.0, 255.0);
    // Falso-positivo pedantic: `v` provadamente em `0..255` após `clamp`.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let out = v as u8;
    out
}

/// `usize` → `i64` saturando (ex.: largura de área).
#[must_use]
pub fn i64_from_usize(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// `u64` → `i64` saturando (ex.: `tick` do shimmer).
#[must_use]
pub fn i64_from_u64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// Células preenchidas da barra (`ratio` `0..1`, `width` células).
///
/// Trunca como o `as usize` original — não arredonda, para não
/// mudar pixels.
#[must_use]
pub fn filled_cells(ratio: f64, width: usize) -> usize {
    let w = f64_from_usize(width);
    let v = (ratio.clamp(0.0, 1.0) * w).clamp(0.0, w);
    // Falso-positivo pedantic: `v` provadamente em `0..=width` após `clamp`.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let out = v as usize;
    out
}

/// Posição do brilho da barra a partir do `tick`.
///
/// `saturating_mul` evita pânico em debug após ~4 anos a 30fps;
/// em uso real é idêntico ao `* 2` original.
#[must_use]
pub fn tick_head(tick: u64, width: usize) -> usize {
    let t = usize::try_from(tick).unwrap_or(usize::MAX);
    t.saturating_mul(2) % width.max(1)
}

/// Percentual `0..100` a partir de `ratio` `0..1`.
///
/// Trunca como o `as u16` original.
#[must_use]
pub fn percent_u16(ratio: f64) -> u16 {
    let v = (ratio.clamp(0.0, 1.0) * 100.0).clamp(0.0, 100.0);
    // Falso-positivo pedantic: `v` provadamente em `0..100` após `clamp`.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let out = v as u16;
    out
}

/// Intensidade 0.0–1.0 da onda triangular centrada em `center`.
fn intensity(index: i64, center: i64) -> f64 {
    let d = f64_from_i64(index.wrapping_sub(center).abs());
    (1.0 - d / f64_from_i64(BAND)).clamp(0.0, 1.0)
}

/// Mistura duas cores por `t` (0 = base, 1 = highlight).
fn mix(base: Color, hi: Color, t: f64) -> Color {
    let (br, bg, bb) = rgb(base);
    let (hr, hg, hb) = rgb(hi);
    let m = |b: u8, h: u8| u8_from_f64_clamped(f64::from(b) + (f64::from(h) - f64::from(b)) * t);
    Color::Rgb(m(br, hr), m(bg, hg), m(bb, hb))
}

fn rgb(c: Color) -> (u8, u8, u8) {
    if let Color::Rgb(r, g, b) = c {
        (r, g, b)
    } else {
        (139, 148, 158)
    }
}

/// Texto com shimmer varrendo da esquerda para a direita.
///
/// # Exemplos
///
/// ```rust
/// use prt::tui::shimmer::shimmer_text;
/// let line = shimmer_text("gerando…", 0, 20);
/// assert!(!line.spans.is_empty());
/// ```
#[must_use]
pub fn shimmer_text(text: &str, tick: u64, width: usize) -> Line<'static> {
    // Sem cor: mesma estrutura (um span por char), tudo em `muted` sem brilho.
    if !colors_enabled() {
        let plain = theme().muted;
        return Line::from(
            text.chars()
                .map(|ch| Span::styled(ch.to_string(), plain))
                .collect::<Vec<_>>(),
        );
    }
    let w = i64_from_usize(width.max(text.chars().count().max(1)));
    let center = i64_from_u64(tick).saturating_mul(2) % (w + BAND * 2) - BAND;
    let base = theme().muted.fg.unwrap_or(Color::Rgb(139, 148, 158));
    let hi = Color::White;
    let spans: Vec<Span<'static>> = text
        .chars()
        .enumerate()
        .map(|(i, ch)| {
            let t = intensity(i64_from_usize(i), center.min(w));
            let style = if t > 0.02 {
                Style::new()
                    .fg(mix(base, hi, t))
                    .add_modifier(Modifier::BOLD)
            } else {
                theme().muted
            };
            Span::styled(ch.to_string(), style)
        })
        .collect();
    Line::from(spans)
}

/// Barra de progresso custom com shimmer na região preenchida.
///
/// Retorna `Line` com `width` células: `█` preenchido, `░` vazio, e uma
/// faixa brilhante de 3 células que se move enquanto `active`.
#[must_use]
pub fn shimmer_bar(ratio: f64, width: usize, tick: u64, active: bool) -> Line<'static> {
    let width = width.max(8);
    let filled = filled_cells(ratio, width);
    let head = if active {
        tick_head(tick, width)
    } else {
        usize::MAX // sem brilho quando inativo
    };
    let mut spans = Vec::with_capacity(width);
    for i in 0..width {
        let is_filled = i < filled;
        // Brilho próximo ao head (faixa de 3) — só com cor.
        let glow =
            active && colors_enabled() && i.abs_diff(head) <= 1 && (is_filled || filled == 0);
        let style = if glow {
            Style::new().fg(Color::White).add_modifier(Modifier::BOLD)
        } else if is_filled {
            theme().accent
        } else if colors_enabled() {
            Style::new().fg(Color::Rgb(48, 54, 61))
        } else {
            Style::new()
        };
        spans.push(Span::styled(if is_filled { "█" } else { "░" }, style));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shimmer_should_span_full_text() {
        let line = shimmer_text("abc", 5, 10);
        assert_eq!(line.spans.len(), 3);
    }

    #[test]
    fn bar_should_have_exact_width() {
        let line = shimmer_bar(0.5, 20, 3, true);
        assert_eq!(line.spans.len(), 20);
    }

    #[test]
    fn bar_inactive_should_not_glow_white() {
        let line = shimmer_bar(0.5, 10, 0, false);
        for sp in &line.spans {
            assert_ne!(sp.style.fg, Some(Color::White));
        }
    }
}
