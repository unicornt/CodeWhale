//! Shared chip span builders for the header and footer.
//!
//! These previously lived as `&self` methods on `HeaderWidget`. Phase 3 of
//! the Claude-style migration moves the visible information into the
//! footer, so both surfaces need to call the same span builders. To keep
//! the header tests passing, the methods are still exposed there as thin
//! delegates while the footer reads these directly.
//!
//! Some helpers (effort_chip / provider_chip / percent_only) are wired up
//! in phase 4 once the header is removed entirely; allow dead code now to
//! avoid scattering `#[allow]` across each declaration.

#![allow(dead_code)]

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

use crate::palette;

pub const CONTEXT_WARNING_THRESHOLD_PERCENT: f64 = 85.0;
pub const CONTEXT_CRITICAL_THRESHOLD_PERCENT: f64 = 95.0;

#[must_use]
pub fn context_percent(last_prompt_tokens: Option<u32>, context_window: Option<u32>) -> Option<f64> {
    let used = f64::from(last_prompt_tokens?);
    let max = f64::from(context_window?);
    if max <= 0.0 {
        return None;
    }
    Some((used / max * 100.0).clamp(0.0, 100.0))
}

#[must_use]
pub fn context_color(percent: f64) -> Color {
    if percent >= CONTEXT_CRITICAL_THRESHOLD_PERCENT {
        palette::STATUS_ERROR
    } else if percent >= CONTEXT_WARNING_THRESHOLD_PERCENT {
        palette::STATUS_WARNING
    } else {
        palette::DEEPSEEK_SKY
    }
}

#[must_use]
pub fn context_signal_spans(
    last_prompt_tokens: Option<u32>,
    context_window: Option<u32>,
    _show_percent: bool,
) -> Vec<Span<'static>> {
    let Some(percent) = context_percent(last_prompt_tokens, context_window) else {
        return Vec::new();
    };

    // Claude Code chrome-light footer reads `64% context left`. We mirror
    // it: a single muted text chip with the *remaining* percentage and the
    // `context left` suffix.
    //
    // Showing the remaining (not used) percentage matches the user's mental
    // model — "I have N% headroom before I hit the window" — and avoids the
    // visual paradox the old `36% ████░░░░` chip produced (a 36%-used chip
    // that visually painted ~half the bar full because rounding + East
    // Asian Ambiguous glyphs both inflated the filled count).
    //
    // The colour still escalates through warning/critical when usage rises,
    // so users see the chip turn yellow/red as they approach the cap.
    let remaining = (100.0 - percent).clamp(0.0, 100.0);
    let color = context_color(percent);
    vec![Span::styled(
        format!("{remaining:.0}% context left"),
        Style::default().fg(color),
    )]
}

#[must_use]
pub fn context_percent_spans(
    last_prompt_tokens: Option<u32>,
    context_window: Option<u32>,
) -> Vec<Span<'static>> {
    let Some(percent) = context_percent(last_prompt_tokens, context_window) else {
        return Vec::new();
    };

    vec![Span::styled(
        format!("{percent:.0}%"),
        Style::default().fg(context_color(percent)),
    )]
}

#[must_use]
pub fn status_indicator_spans(frame: Option<&'static str>) -> Vec<Span<'static>> {
    let Some(frame) = frame else {
        return Vec::new();
    };
    vec![Span::styled(
        frame.to_string(),
        Style::default().fg(palette::DEEPSEEK_SKY),
    )]
}

#[must_use]
pub fn provider_chip_spans(provider_label: Option<&str>) -> Vec<Span<'static>> {
    let Some(label) = provider_label else {
        return Vec::new();
    };
    let trimmed = label.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    vec![Span::styled(
        trimmed.to_string(),
        Style::default()
            .fg(palette::DEEPSEEK_SKY)
            .add_modifier(Modifier::BOLD),
    )]
}

#[must_use]
pub fn effort_chip_spans(label: Option<&str>, include_prefix: bool) -> Vec<Span<'static>> {
    let Some(label) = label else {
        return Vec::new();
    };
    let trimmed = label.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let is_off = trimmed.eq_ignore_ascii_case("off");
    let color = if is_off {
        palette::TEXT_HINT
    } else {
        palette::DEEPSEEK_SKY
    };
    let body = if !include_prefix {
        trimmed.to_string()
    } else if trimmed.eq_ignore_ascii_case("max") || trimmed.eq_ignore_ascii_case("maximum") {
        // Use a non-emoji diamond (U+25C6, always 1 column) instead of an
        // SMP emoji whose rendered width is inconsistent across terminals.
        format!("\u{25C6} {trimmed}")
    } else {
        format!("\u{00B7} {trimmed}")
    };
    vec![Span::styled(body, Style::default().fg(color))]
}

#[must_use]
pub fn live_marker_spans(show_label: bool) -> Vec<Span<'static>> {
    let mut spans = vec![Span::styled(
        "●",
        Style::default()
            .fg(palette::DEEPSEEK_SKY)
            .add_modifier(Modifier::BOLD),
    )];
    if show_label {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            "Live",
            Style::default().fg(palette::TEXT_SOFT),
        ));
    }
    spans
}

#[must_use]
pub fn version_label() -> String {
    format!("v{}", env!("CARGO_PKG_VERSION"))
}

#[must_use]
pub fn version_spans() -> Vec<Span<'static>> {
    vec![Span::styled(
        version_label(),
        Style::default().fg(palette::TEXT_HINT),
    )]
}
