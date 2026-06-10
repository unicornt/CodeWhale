//! Footer bar widget displaying mode, status, model, and auxiliary chips.
//!
//! `FooterWidget` is a pure render of a [`FooterProps`] struct: all content
//! (labels, colors, span clusters) is computed once per redraw at a higher
//! level, then `FooterWidget::new(props).render(area, buf)` paints the
//! result. The widget owns no `App` knowledge; this mirrors the layout used
//! by `HeaderWidget` (and Codex's `bottom_pane::footer::Footer`).

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::localization::{Locale, MessageId, tr};
use crate::palette;
use crate::tui::app::{App, AppMode};

use super::Renderable;

/// Pre-computed data the footer needs to render.
///
/// All fields are owned `String` / `Vec<Span<'static>>` values so the props
/// can be built once per redraw and then handed to a borrow-free widget.
#[derive(Debug, Clone)]
pub struct FooterProps {
    /// The current model identifier shown after the mode chip.
    pub model: String,
    /// `"agent"` / `"yolo"` / `"plan"` — the canonical setting label.
    pub mode_label: &'static str,
    /// Color used for the mode chip.
    pub mode_color: Color,
    /// Color used for small separators between chips.
    pub text_dim_color: Color,
    /// Color used for the model label.
    pub text_hint_color: Color,
    /// Color used for steady secondary chips such as cost.
    pub text_muted_color: Color,
    /// Background color for the full footer/status bar row.
    pub footer_bg: Color,
    /// Status label like `"ready"`, `"thinking ⌫"`, `"working"`. When the
    /// label equals `"ready"` the footer hides the status segment entirely.
    pub state_label: String,
    /// Color used for the status label.
    pub state_color: Color,
    /// Coherence chip spans (empty when no active intervention).
    pub coherence: Vec<Span<'static>>,
    /// Sub-agent count chip spans (empty when zero in-flight).
    pub agents: Vec<Span<'static>>,
    /// Reasoning-replay chip spans (empty when zero / not applicable).
    pub reasoning_replay: Vec<Span<'static>>,
    /// Cache-hit-rate chip spans (empty when no usage reported).
    pub cache: Vec<Span<'static>>,
    /// MCP server health chip spans (empty when no MCP servers configured).
    /// Populated lazily — see [`footer_mcp_chip`]. (#502)
    pub mcp: Vec<Span<'static>>,
    /// Cumulative model-work chip spans ("worked 3h 12m"). Sums the
    /// elapsed time of completed turns (from `App::cumulative_turn_duration`),
    /// **not** wall-clock since launch — an idle TUI shouldn't claim
    /// it's been "working." Empty until cumulative turn time crosses
    /// 60s. Populated by [`footer_worked_chip`]. (#448)
    pub worked: Vec<Span<'static>>,
    /// Snapshot of the global retry-status surface (#499). Sampled once
    /// at props-build time and rendered as a foreground banner on the
    /// left of the footer when active. Captured here (rather than read
    /// from `retry_status` at render time) so tests can pin a
    /// deterministic state without racing the parallel runner.
    pub retry: crate::retry_status::RetryState,
    /// Session-cost chip spans (empty when below the display threshold).
    /// Rendered in the left cluster (after the model name) — cost is steady
    /// info, not a transient signal, so it lives with mode and model.
    pub cost: Vec<Span<'static>>,
    /// Account balance chip spans (empty when un fetched or zero). Rendered
    /// in the left cluster right after cost.
    pub balance: Vec<Span<'static>>,
    /// Context-utilisation chip spans (e.g. `▰▰▱▱ 23%`). Empty when there
    /// is no `last_prompt_tokens` value yet. Rendered in the left cluster
    /// just after the model/provider so the user can keep an eye on the
    /// remaining context budget without leaving the bottom row.
    pub ctx: Vec<Span<'static>>,
    /// Status-indicator frame (whale or dots) drawn in the centre between
    /// the left and right clusters during a live turn. `None` when the
    /// indicator is disabled or the app is idle.
    pub status_indicator: Option<&'static str>,
    /// `● Live` streaming marker — phase 4 brings this back into the footer
    /// centre next to the status indicator (the persistent header used to
    /// host the duplicate; with the header gone, the centre is the only
    /// surface that signals streaming).
    pub live_marker: Vec<Span<'static>>,
    /// Build/version chip spans (e.g. `v0.8.53`) — always present, drops
    /// last under width pressure.
    pub version: Vec<Span<'static>>,
    /// Transient turn receipts (token / cost summary) — rendered in the
    /// right cluster just before the version chip. Empty when no recent
    /// turn finished within the receipt TTL.
    pub receipts: Vec<Span<'static>>,
    /// Optional toast that, when present, replaces the left status line.
    pub toast: Option<FooterToast>,
}

/// Pulse the localized "working" label through 0–3 trailing ASCII dots
/// keyed off `frame`. The cycle period is 4 frames (matching the four
/// states), so adjacent ticks visibly differ. Dots stay ASCII regardless
/// of locale so the animation reads identically across scripts. Returns a
/// `String` so callers can drop it into a `Span::styled` without lifetime
/// gymnastics.
#[must_use]
pub fn footer_working_label(frame: u64, locale: Locale) -> String {
    let dots = (frame % 4) as usize;
    let base = tr(locale, MessageId::FooterWorking);
    let mut out = String::with_capacity(base.len() + dots);
    out.push_str(base);
    for _ in 0..dots {
        out.push('.');
    }
    out
}

/// Build a "⏳ shell running" chip span when a foreground shell command is
/// active. Empty when no shell is running, which hides the chip entirely.
#[must_use]
pub fn footer_shell_chip(active: bool) -> Vec<Span<'static>> {
    if !active {
        return Vec::new();
    }
    vec![Span::styled(
        "\u{23F3} shell running".to_string(),
        Style::default().fg(palette::STATUS_WARNING),
    )]
}

/// Build a "N agents" chip span list when there are sub-agents in flight.
/// Empty list when N == 0 hides the chip entirely. Singular for N == 1
/// reads naturally; plural otherwise. The pluralization template lives in
/// the locale registry so CJK locales can render the count without the
/// English plural-`s` artefact.
#[must_use]
#[allow(dead_code)] // sidebar tasks panel now shows active agents; footer
                    // chip dropped, but the function stays compiled because
                    // a handful of regression tests still call it directly.
pub fn footer_agents_chip(running: usize, locale: Locale) -> Vec<Span<'static>> {
    if running == 0 {
        return Vec::new();
    }
    let text = if running == 1 {
        tr(locale, MessageId::FooterAgentSingular).to_string()
    } else {
        tr(locale, MessageId::FooterAgentsPlural).replace("{count}", &running.to_string())
    };
    vec![Span::styled(
        text,
        Style::default().fg(palette::DEEPSEEK_SKY),
    )]
}

/// Build the cumulative-elapsed chip ("worked 3h 12m") for the
/// footer's right cluster (#448). Hidden during the first minute of
/// a session so a fresh launch doesn't render a noisy `worked 5s`
/// indicator that immediately starts ticking. Above the threshold,
/// reuses [`crate::tui::notifications::humanize_duration`] for
/// consistent w/d/h/m formatting.
#[must_use]
pub fn footer_worked_chip(elapsed: std::time::Duration) -> Vec<Span<'static>> {
    if elapsed < std::time::Duration::from_secs(60) {
        return Vec::new();
    }
    let label = format!(
        "worked {}",
        crate::tui::notifications::humanize_duration(elapsed)
    );
    vec![Span::styled(
        label,
        Style::default().fg(palette::TEXT_MUTED),
    )]
}

/// Build the "MCP M/N" health chip (#502) from the user's stored
/// snapshot. `connected` is the number of servers currently reachable;
/// `configured` is the number declared in the user's MCP config. When
/// `configured` is zero the chip is hidden entirely.
///
/// Colour-codes the count by health:
/// - all reachable → success
/// - some reachable → warning
/// - none reachable but at least one configured → error
/// - configured but no live snapshot yet → muted (count only)
#[must_use]
pub fn footer_mcp_chip(connected: Option<usize>, configured: usize) -> Vec<Span<'static>> {
    if configured == 0 {
        return Vec::new();
    }
    let (label, color) = match connected {
        None => (format!("MCP {configured}"), palette::TEXT_MUTED),
        Some(c) if c == configured => (format!("MCP {c}/{configured}"), palette::STATUS_SUCCESS),
        Some(0) => (format!("MCP 0/{configured}"), palette::STATUS_ERROR),
        Some(c) => (format!("MCP {c}/{configured}"), palette::STATUS_WARNING),
    };
    vec![Span::styled(label, Style::default().fg(color))]
}

/// A status toast routed to the footer's left segment for a short time.
#[derive(Debug, Clone)]
pub struct FooterToast {
    pub text: String,
    pub color: Color,
}

impl FooterProps {
    /// Build footer props from common app state. Helpers in `tui/ui.rs`
    /// (e.g. `footer_state_label`, `footer_coherence_spans`) supply the
    /// pre-styled spans and labels — this constructor just bundles them.
    ///
    /// Argument fan-out is intentional: each input maps 1:1 to a piece of
    /// pre-computed footer content the caller resolved from `App`. Forcing
    /// these into a builder would obscure the call site without making the
    /// data flow any clearer.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn from_app(
        app: &App,
        toast: Option<FooterToast>,
        state_label: &str,
        state_color: Color,
        coherence: Vec<Span<'static>>,
        agents: Vec<Span<'static>>,
        reasoning_replay: Vec<Span<'static>>,
        cache: Vec<Span<'static>>,
        cost: Vec<Span<'static>>,
        balance: Vec<Span<'static>>,
    ) -> Self {
        let (mode_label, mode_color) = mode_style(app);
        // MCP chip (#502) — passive, derived from the user's existing
        // snapshot. `connected` is `None` until the user runs `/mcp`,
        // which is the same trigger the issue spec accepts for now.
        let mcp_configured = app.mcp_configured_count;
        let mcp_connected = app
            .mcp_snapshot
            .as_ref()
            .map(|s| s.servers.iter().filter(|server| server.connected).count());
        let mcp = footer_mcp_chip(mcp_connected, mcp_configured);
        // #448: cumulative work-time chip. Sums actual turn durations
        // (set on `TurnComplete`) rather than wall-clock uptime — a TUI
        // that's been open and idle for 4 minutes shouldn't claim
        // "worked 4m". The chip stays empty until enough turns add up
        // to cross the 60s threshold inside `footer_worked_chip`.
        let worked = footer_worked_chip(app.cumulative_turn_duration);
        // Pull the chips that the legacy `HeaderWidget` exposed — the footer
        // takes over the "always visible / context-utilisation / version /
        // status indicator / live marker" cluster as the header is removed.
        // …except the context-utilisation chip itself: the sidebar's Context
        // panel already carries `context: T/W tokens (X%)`, so we keep the
        // ctx slot empty in the footer to avoid duplicating it.
        let ctx: Vec<Span<'static>> = Vec::new();
        // `context_window` and `last_prompt_tokens` are still consumed by
        // the sidebar; here we just drop the chip on the footer side.
        let _ = crate::models::context_window_for_model(&app.model);
        let status_indicator_started_at = if app.low_motion {
            None
        } else {
            app.turn_started_at
        };
        // Phase 5 follow-up: the status-indicator (whale animation) moved
        // from the centre cluster to the left of `mode_label`. We keep the
        // frame populated *even when idle* — `header_status_indicator_frame`
        // returns the first whale glyph at `turn_started_at = None`, which
        // is the static "ready" pose. That way the whale is always visible
        // as the left-most CodeWhale brand anchor; the cycle only kicks in
        // during live turns.
        let status_indicator = super::header::header_status_indicator_frame(
            status_indicator_started_at,
            &app.status_indicator,
        );
        // Centre cluster keeps only the streaming `●` marker; the whale
        // moved to the left so we don't render it twice in one row.
        let live_marker = if app.is_loading {
            super::chips::live_marker_spans(false)
        } else {
            Vec::new()
        };
        let version = super::chips::version_spans();
        let receipts = if let Some(text) = app.active_receipt_text() {
            vec![Span::styled(
                text.trim().to_string(),
                Style::default()
                    .fg(palette::STATUS_SUCCESS)
                    .add_modifier(ratatui::style::Modifier::DIM),
            )]
        } else {
            Vec::new()
        };
        Self {
            model: app.model_display_label(),
            mode_label,
            mode_color,
            text_dim_color: app.ui_theme.text_dim,
            text_hint_color: app.ui_theme.text_hint,
            text_muted_color: app.ui_theme.text_muted,
            footer_bg: app.ui_theme.footer_bg,
            state_label: state_label.to_string(),
            state_color,
            coherence,
            agents,
            reasoning_replay,
            cache,
            mcp,
            worked,
            cost,
            balance,
            ctx,
            status_indicator,
            live_marker,
            version,
            receipts,
            toast,
            retry: crate::retry_status::snapshot(),
        }
    }
}

fn mode_style(app: &App) -> (&'static str, Color) {
    let label = match app.mode {
        AppMode::Agent => "agent",
        AppMode::Yolo => "yolo",
        AppMode::Plan => "plan",
    };
    let color = match app.mode {
        AppMode::Agent => app.ui_theme.mode_agent,
        AppMode::Yolo => app.ui_theme.mode_yolo,
        AppMode::Plan => app.ui_theme.mode_plan,
    };
    (label, color)
}

/// Pure-render footer. Build once per frame, then `render(area, buf)`.
pub struct FooterWidget {
    props: FooterProps,
}

impl FooterWidget {
    #[must_use]
    pub fn new(props: FooterProps) -> Self {
        Self { props }
    }

    fn auxiliary_spans(&self, max_width: usize) -> Vec<Span<'static>> {
        // `cost` is rendered in the left cluster now — keep it out of the
        // right-hand chip parade. Coherence / agents / replay / cache are
        // transient signals; they belong on the right where they appear and
        // disappear without disturbing the steady mode·model·cost line.
        //
        // `ctx` is the context-utilisation signal taken over from the
        // legacy header. It sits up front (highest priority among
        // right-cluster chips) so users keep an eye on the budget. `worked`
        // and `receipts` are the lowest priority and drop first under
        // narrow widths. The `version` chip closes the right cluster.
        let parts: Vec<&Vec<Span<'static>>> = [
            &self.props.ctx,
            &self.props.coherence,
            &self.props.agents,
            &self.props.reasoning_replay,
            &self.props.cache,
            &self.props.mcp,
            &self.props.receipts,
            &self.props.worked,
            &self.props.version,
        ]
        .into_iter()
        .filter(|spans| !spans.is_empty())
        .collect();

        // Try to fit as many parts as possible, dropping from the end.
        for end in (0..=parts.len()).rev() {
            let mut combined: Vec<Span<'static>> = Vec::new();
            for (i, part) in parts[..end].iter().enumerate() {
                if i > 0 {
                    combined.push(Span::raw("  "));
                }
                combined.extend(part.iter().cloned());
            }
            if span_width(&combined) <= max_width {
                return combined;
            }
        }
        Vec::new()
    }

    fn toast_spans(toast: &FooterToast, max_width: usize) -> Vec<Span<'static>> {
        let truncated = truncate_to_width(&toast.text, max_width.max(1));
        vec![Span::styled(truncated, Style::default().fg(toast.color))]
    }

    /// Build the left status line with priority-ordered hint dropping.
    ///
    /// Priority order (highest to lowest — last to drop):
    /// 1. Mode label (always visible at any width; truncated only as a last resort)
    /// 2. Model name (always visible; then truncated mid-word once all hints are gone)
    /// 3. Balance chip — drops third (account balance is more actionable than session cost)
    /// 4. Cost chip — drops fourth
    /// 5. Status label (e.g. "working", "draft") — drops first when space is tight
    ///
    /// At every width ≥40 cols the line never wraps mid-hint.
    fn status_line_spans(&self, max_width: usize) -> Vec<Span<'static>> {
        if max_width == 0 {
            return Vec::new();
        }

        // Live marker `●` — moved here from the centre cluster so the
        // streaming heartbeat sits directly next to the whale brand glyph.
        // Painted before the whale so the visual order reads
        // "(live dot) (whale) mode · model · …".
        let live_marker_w = if self.props.live_marker.is_empty() {
            0
        } else {
            // `●` (U+25CF) is unambiguously narrow + we always pad with a
            // trailing space, so the cluster width is `width_of_marker + 1`.
            // `span_width` would also report this exact value because the
            // marker is not a wide grapheme.
            span_width(&self.props.live_marker) + 1
        };

        // Whale prefix — phase 5 follow-up: the status-indicator moved from
        // the centre cluster to the left so the footer always carries the
        // CodeWhale anchor. Idle = static first frame, in-flight = animated.
        //
        // Width budget: `unicode_width(whale)` + 1 phantom (ratatui paints a
        // continuation cell `" "` after every wide glyph) + 1 explicit
        // separator space we add at render time. For a 2-col emoji that's
        // `2 + 1 + 1 = 4`. `span_width` independently computes the same 4
        // because it adds 1 per wide grapheme on top of `UnicodeWidthStr`.
        let whale_glyph = self.props.status_indicator.unwrap_or("");
        let whale_glyph_w = UnicodeWidthStr::width(whale_glyph);
        let whale_prefix_w = if whale_glyph.is_empty() {
            0
        } else if whale_glyph_w >= 2 {
            // Wide emoji (🐳): 2 cols + 1 phantom + 1 explicit space.
            whale_glyph_w + 2
        } else {
            // Narrow indicator (dots ◍/◉/◌): 1 col + 1 explicit space.
            whale_glyph_w + 1
        };

        let prefix_w = live_marker_w + whale_prefix_w;

        let mode_label = self.props.mode_label;
        let sep = " \u{00B7} ";
        let model = self.props.model.as_str();
        let show_status = self.props.state_label != "ready";
        let status_label = self.props.state_label.as_str();
        let cost_text = spans_text(&self.props.cost);
        let show_cost = !cost_text.is_empty();
        let balance_text = spans_text(&self.props.balance);
        let show_balance = !balance_text.is_empty();

        let mode_w = mode_label.width();
        let sep_w = sep.width();
        let model_w = UnicodeWidthStr::width(model);
        let status_w = if show_status { status_label.width() } else { 0 };
        let cost_w = if show_cost { cost_text.width() } else { 0 };
        let balance_w = if show_balance {
            balance_text.width()
        } else {
            0
        };

        let extra_sep = |w: usize| if w > 0 { sep_w } else { 0 };

        // Tier 1: live whale mode · model · balance · cost · status
        let full_w = prefix_w
            + mode_w
            + sep_w
            + model_w
            + extra_sep(balance_w)
            + balance_w
            + extra_sep(cost_w)
            + cost_w
            + extra_sep(status_w)
            + status_w;
        if (show_balance || show_cost || show_status) && full_w <= max_width {
            return self.build_status_line_spans(
                mode_label,
                model.to_string(),
                show_balance.then(|| balance_text.clone()),
                show_cost.then(|| cost_text.clone()),
                show_status.then_some(status_label),
            );
        }

        // Tier 2: live whale mode · model · balance · cost — drop status.
        let with_cost_w = prefix_w
            + mode_w
            + sep_w
            + model_w
            + extra_sep(balance_w)
            + balance_w
            + extra_sep(cost_w)
            + cost_w;
        if (show_balance || show_cost) && with_cost_w <= max_width {
            return self.build_status_line_spans(
                mode_label,
                model.to_string(),
                show_balance.then(|| balance_text.clone()),
                show_cost.then(|| cost_text.clone()),
                None,
            );
        }

        // Tier 3: live whale mode · model · balance — drop cost.
        if show_balance {
            let with_balance_w = prefix_w + mode_w + sep_w + model_w + sep_w + balance_w;
            if with_balance_w <= max_width {
                return self.build_status_line_spans(
                    mode_label,
                    model.to_string(),
                    Some(balance_text.clone()),
                    None,
                    None,
                );
            }
        }

        // Tier 4: live whale mode · model — drop balance too.
        let mode_model_w = prefix_w + mode_w + sep_w + model_w;
        if mode_model_w <= max_width {
            return self.build_status_line_spans(mode_label, model.to_string(), None, None, None);
        }

        // Tier 5: live whale mode · <truncated model> — keep both labels visible
        // by ellipsizing the model name. Only do this when there is enough room
        // for at least the ellipsis ("..."). Below that we drop to mode-only.
        let tier5_left_w = prefix_w + mode_w + sep_w;
        if tier5_left_w < max_width {
            let model_budget = max_width - tier5_left_w;
            if model_budget >= 4 {
                let truncated = truncate_to_width(model, model_budget);
                if !truncated.is_empty() {
                    return self.build_status_line_spans(mode_label, truncated, None, None, None);
                }
            }
        }

        // Tier 6: live whale mode-only.
        let mode_only_w = prefix_w + mode_w;
        if mode_only_w <= max_width {
            return self.build_status_line_spans(mode_label, String::new(), None, None, None);
        }

        // Tier 7: whale only — sub-mode-width row, drop the label too. The
        // whale alone is still a useful "alive" signal at very narrow widths.
        if !whale_glyph.is_empty() && UnicodeWidthStr::width(whale_glyph) <= max_width {
            return vec![Span::styled(
                whale_glyph.to_string(),
                Style::default().fg(palette::ACCENT_PRIMARY),
            )];
        }

        // Tier 8: terminal so narrow even the whale glyph won't fit. Fall
        // back to a truncated mode label so we never produce empty output.
        vec![Span::styled(
            truncate_to_width(mode_label, max_width),
            Style::default().fg(self.props.mode_color),
        )]
    }

    fn build_status_line_spans(
        &self,
        mode_label: &'static str,
        model_label: String,
        balance: Option<String>,
        cost: Option<String>,
        status: Option<&str>,
    ) -> Vec<Span<'static>> {
        let sep = " \u{00B7} ";
        let mut spans: Vec<Span<'static>> = Vec::new();

        // Live marker — when in flight, the streaming heartbeat sits before
        // the whale so the visual order reads "● 🐳 mode · model · …".
        // Idle turns leave this empty.
        if !self.props.live_marker.is_empty() {
            spans.extend(self.props.live_marker.iter().cloned());
            spans.push(Span::raw(" "));
        }

        // Whale prefix — always emitted when the props carry a frame. We
        // always paint an explicit trailing space (rather than relying on
        // ratatui's wide-glyph continuation cell) so the brand glyph reads
        // as a deliberate separator, not a fused part of `mode_label`.
        if let Some(frame) = self.props.status_indicator {
            spans.push(Span::styled(
                frame.to_string(),
                Style::default().fg(palette::ACCENT_PRIMARY),
            ));
            spans.push(Span::raw(" "));
        }

        if !mode_label.is_empty() {
            spans.push(Span::styled(
                mode_label.to_string(),
                Style::default().fg(self.props.mode_color),
            ));
        }
        if !model_label.is_empty() {
            // Only the *non-whale* spans participate in the `· ` separator
            // dance — the whale prefix is delimited by its own trailing
            // space, not the bullet.
            let has_label_before = spans
                .iter()
                .any(|s| s.style.fg == Some(self.props.mode_color));
            if has_label_before {
                spans.push(Span::styled(
                    sep.to_string(),
                    Style::default().fg(self.props.text_dim_color),
                ));
            }
            spans.push(Span::styled(
                model_label,
                Style::default().fg(self.props.text_hint_color),
            ));
        }
        if let Some(balance_text) = balance {
            spans.push(Span::styled(
                sep.to_string(),
                Style::default().fg(self.props.text_dim_color),
            ));
            spans.push(Span::styled(
                balance_text,
                Style::default().fg(self.props.text_muted_color),
            ));
        }
        if let Some(cost_text) = cost {
            spans.push(Span::styled(
                sep.to_string(),
                Style::default().fg(self.props.text_dim_color),
            ));
            spans.push(Span::styled(
                cost_text,
                Style::default().fg(self.props.text_muted_color),
            ));
        }
        if let Some(status_label) = status {
            spans.push(Span::styled(
                sep.to_string(),
                Style::default().fg(self.props.text_dim_color),
            ));
            spans.push(Span::styled(
                status_label.to_string(),
                Style::default().fg(self.props.state_color),
            ));
        }
        spans
    }

    /// Build the left status line.
    ///
    /// Priority order — only one of these renders at a time:
    /// 1. Retry banner (#499) — connection-level failure, must surface
    ///    above anything else.
    /// 2. Toast — transient notifications take the line for their TTL.
    /// 3. Status line (mode · model · ... · status_label) — steady state.
    fn left_spans(&self, max_width: usize) -> Vec<Span<'static>> {
        if let Some(banner) = retry_banner_spans(max_width, &self.props) {
            banner
        } else if let Some(toast) = self.props.toast.as_ref() {
            Self::toast_spans(toast, max_width)
        } else {
            self.status_line_spans(max_width)
        }
    }
}

fn spans_text(spans: &[Span<'_>]) -> String {
    spans.iter().map(|s| s.content.as_ref()).collect::<String>()
}

/// Render the retry banner (#499) when the props' captured snapshot
/// reports an active retry or a final failure. Returns `None` when idle
/// so callers fall back to the regular status line / toast.
fn retry_banner_spans(max_width: usize, props: &FooterProps) -> Option<Vec<Span<'static>>> {
    let (label, color) = match &props.retry {
        crate::retry_status::RetryState::Active(banner) => {
            let secs = props.retry.seconds_remaining().unwrap_or(0);
            // Round to 1s — we redraw each frame anyway so the
            // countdown ticks visually without us having to schedule
            // anything extra.
            (
                format!("⟳ retry {} in {secs}s — {}", banner.attempt, banner.reason),
                crate::palette::STATUS_WARNING,
            )
        }
        crate::retry_status::RetryState::Failed { reason, .. } => {
            (format!("× failed: {reason}"), crate::palette::STATUS_ERROR)
        }
        crate::retry_status::RetryState::Idle => return None,
    };
    let truncated = truncate_to_width(&label, max_width);
    Some(vec![Span::styled(truncated, Style::default().fg(color))])
}

impl Renderable for FooterWidget {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        let available_width = area.width as usize;
        if available_width == 0 {
            return;
        }

        // Clear the whole footer row first so stale transcript glyphs from
        // the previous frame cannot survive in cells this frame's spans do not
        // touch (#2244).
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                buf[(x, y)]
                    .set_symbol(" ")
                    .set_style(Style::default().bg(self.props.footer_bg));
            }
        }

        let preview_left_spans = self.left_spans(available_width);
        let preview_left_width = span_width(&preview_left_spans);
        let right_budget = available_width
            .saturating_sub(preview_left_width)
            .saturating_sub(2);
        let mut right_spans = self.auxiliary_spans(right_budget);
        let mut right_width = span_width(&right_spans);
        let min_gap = if right_width > 0 { 2 } else { 0 };
        let max_left_width = available_width
            .saturating_sub(right_width)
            .saturating_sub(min_gap)
            .max(1);
        let left_spans = self.left_spans(max_left_width);
        let left_width = span_width(&left_spans);

        // Safety net: if the actual left + right cluster exceeds the row
        // width, peel chips off the right cluster until it fits. This
        // catches the case where `left_spans` re-expanded after the right
        // cluster was already chosen on the preview width — without this,
        // the paragraph paints up to the right edge and the trailing right
        // chips get clipped (the user-visible "ctx/version overflow" bug).
        while left_width + min_gap + right_width > available_width && !right_spans.is_empty() {
            right_spans.pop();
            // Drop the trailing 2-space separator we inserted between chips
            // alongside its chip so we don't leave a dangling double-space.
            if right_spans
                .last()
                .map(|s| s.content.as_ref() == "  ")
                .unwrap_or(false)
            {
                right_spans.pop();
            }
            right_width = span_width(&right_spans);
        }
        // Still too wide? Truncate the *left* cluster — the spec keeps the
        // mode/model/cost line as the highest-priority info, so peel from
        // the tail (status label / detail hint) until it fits.
        let mut left_spans = if left_width + min_gap + right_width > available_width {
            self.left_spans(
                available_width
                    .saturating_sub(right_width)
                    .saturating_sub(min_gap)
                    .max(1),
            )
        } else {
            left_spans
        };
        let mut left_width = span_width(&left_spans);
        // Final hard clamp: if even the truncated left plus right still
        // overflows (rare but possible when the cascade can't shrink
        // further), drop right chips one more time.
        while left_width + min_gap + right_width > available_width && !right_spans.is_empty() {
            right_spans.pop();
            if right_spans
                .last()
                .map(|s| s.content.as_ref() == "  ")
                .unwrap_or(false)
            {
                right_spans.pop();
            }
            right_width = span_width(&right_spans);
        }
        // Absolute last resort — truncate the left cluster's last span
        // character-by-character so the line never extends past area.
        while left_width + right_width > available_width {
            let Some(last) = left_spans.pop() else {
                break;
            };
            let last_text = last.content.as_ref();
            let allow = available_width
                .saturating_sub(right_width)
                .saturating_sub(span_width(&left_spans));
            if allow == 0 {
                left_width = span_width(&left_spans);
                break;
            }
            let mut accum = String::new();
            let mut used = 0usize;
            for ch in last_text.chars() {
                let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
                if used + cw > allow {
                    break;
                }
                accum.push(ch);
                used += cw;
            }
            if !accum.is_empty() {
                left_spans.push(Span::styled(accum, last.style));
            }
            left_width = span_width(&left_spans);
            break;
        }
        let spacer_width = available_width.saturating_sub(left_width + right_width);

        // Centre cluster: empty since the live `●` streaming marker moved to
        // the left of the whale (so the visual order reads
        // "● 🐳 mode · model · …"). We keep the variable for symmetry with
        // the previous render path, but the spacer is now plain whitespace.
        let mut all_spans = left_spans;
        all_spans.push(Span::raw(" ".repeat(spacer_width)));
        all_spans.extend(right_spans);

        let paragraph =
            Paragraph::new(Line::from(all_spans)).style(Style::default().bg(self.props.footer_bg));
        paragraph.render(area, buf);
    }

    fn desired_height(&self, _width: u16) -> u16 {
        1
    }
}

fn span_width(spans: &[Span<'_>]) -> usize {
    // Account for ratatui's wide-glyph continuation cell. When a span
    // contains an emoji or other 2-col character, ratatui paints the
    // continuation cell with a literal `" "` symbol. Tests collect
    // `(0..area.width).map(|x| buf[(x,0)].symbol())` and check
    // `line.width()`, which includes that phantom space. To keep the
    // budget arithmetic consistent with what tests measure (and what
    // users see in their terminal), we add 1 per wide-grapheme.
    spans
        .iter()
        .map(|span| {
            let content = span.content.as_ref();
            let base = UnicodeWidthStr::width(content);
            let wide_count = content
                .chars()
                .filter(|ch| {
                    UnicodeWidthChar::width(*ch).unwrap_or(0) >= 2
                })
                .count();
            base + wide_count
        })
        .sum()
}

fn truncate_to_width(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(text) <= max_width {
        return text.to_string();
    }
    if max_width <= 3 {
        return text.chars().take(max_width).collect();
    }

    let mut out = String::new();
    let mut width = 0usize;
    let limit = max_width.saturating_sub(3);
    for ch in text.chars() {
        let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + ch_width > limit {
            break;
        }
        out.push(ch);
        width += ch_width;
    }
    out.push_str("...");
    out
}

#[cfg(test)]
mod tests {
    use super::{FooterProps, FooterToast, FooterWidget, Renderable};
    use crate::config::Config;
    use crate::localization::Locale;
    use crate::palette;
    use crate::tui::app::{App, AppMode, TuiOptions};
    use ratatui::{
        buffer::Buffer,
        layout::Rect,
        style::{Color, Style},
        text::Span,
    };
    use std::path::PathBuf;
    use unicode_width::UnicodeWidthStr;

    fn make_app() -> App {
        let options = TuiOptions {
            model: "deepseek-v4-flash".to_string(),
            workspace: PathBuf::from("."),
            config_path: None,
            config_profile: None,
            allow_shell: false,
            use_alt_screen: true,
            use_mouse_capture: false,
            use_bracketed_paste: true,
            max_subagents: 1,
            skills_dir: PathBuf::from("."),
            memory_path: PathBuf::from("memory.md"),
            notes_path: PathBuf::from("notes.txt"),
            mcp_config_path: PathBuf::from("mcp.json"),
            use_memory: false,
            start_in_agent_mode: true,
            skip_onboarding: true,
            yolo: false,
            resume_session_id: None,
            initial_input: None,
        };
        let mut app = App::new(options, &Config::default());
        // App::new may pick up local Settings, which override the option
        // above. Pin model state explicitly so these tests are host-neutral.
        app.model = "deepseek-v4-flash".to_string();
        app.auto_model = false;
        app.api_provider = crate::config::ApiProvider::Deepseek;
        // Same for theme: tests below assert against the default dark palette,
        // but App::new honors saved settings.toml values on the host machine.
        app.theme_id = crate::palette::ThemeId::Whale;
        app.ui_theme = crate::palette::UI_THEME;
        app
    }

    fn idle_props_for(app: &App) -> FooterProps {
        let mut props = FooterProps::from_app(
            app,
            None,
            "ready",
            palette::TEXT_MUTED,
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
        );
        // `from_app` reads the process-wide retry-status surface; pin
        // `Idle` so footer tests don't pick up state set by retry-banner
        // tests running in parallel.
        props.retry = crate::retry_status::RetryState::Idle;
        props
    }

    #[test]
    fn from_app_idle_state_carries_ready_label_and_no_chips() {
        let app = make_app();
        let props = idle_props_for(&app);

        assert_eq!(props.state_label, "ready");
        assert_eq!(props.state_color, palette::TEXT_MUTED);
        assert_eq!(props.mode_label, "agent");
        assert_eq!(props.mode_color, palette::MODE_AGENT);
        assert_eq!(props.text_dim_color, palette::TEXT_DIM);
        assert_eq!(props.text_hint_color, palette::TEXT_HINT);
        assert_eq!(props.text_muted_color, palette::TEXT_MUTED);
        assert_eq!(props.model, "deepseek-v4-flash");
        assert!(props.coherence.is_empty());
        assert!(props.agents.is_empty());
        assert!(props.cache.is_empty());
        assert!(props.cost.is_empty());
        assert!(props.reasoning_replay.is_empty());
        // #448: fresh apps don't get a `worked` chip until completed
        // turns have added up to >= 60s of model work. A freshly-built
        // App has cumulative_turn_duration == 0 so the chip is empty.
        assert!(props.worked.is_empty());
        assert!(props.toast.is_none());
    }

    #[test]
    fn worked_chip_tracks_completed_turn_time_not_session_uptime() {
        // Regression test for the v0.8.8 takedown: the chip used to
        // read `App::session_started_at.elapsed()`, so a TUI that had
        // been open and idle for several minutes claimed "worked 3m"
        // even though no turn had ever fired. The chip now sources
        // from `App::cumulative_turn_duration`, which is only ever
        // incremented on `TurnComplete`. Pin both directions:
        //
        //   1. cumulative == 0 (no turn finished yet)  → empty
        //   2. cumulative crosses 60s (real work)      → label shows
        //   3. wall-clock since launch is irrelevant   → not consulted
        let mut app = make_app();
        // The whole point: cumulative_turn_duration starts at zero,
        // so however long the TUI has been open the chip stays empty
        // until a turn actually completes and adds time.
        let props = idle_props_for(&app);
        assert!(
            props.worked.is_empty(),
            "idle app with zero cumulative turn time must not show worked chip"
        );

        // A real turn finishes for 90s of model work — chip lights up.
        // (`humanize_duration` keeps both units when both are non-zero,
        // so 90s renders as `1m 30s`, not `1m`.)
        app.cumulative_turn_duration = std::time::Duration::from_secs(90);
        let props = idle_props_for(&app);
        let text: String = props
            .worked
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<String>();
        assert_eq!(text, "worked 1m 30s");
    }

    #[test]
    fn footer_worked_chip_hidden_below_one_minute() {
        use std::time::Duration;
        for secs in [0, 1, 30, 59] {
            let chip = super::footer_worked_chip(Duration::from_secs(secs));
            assert!(
                chip.is_empty(),
                "worked chip must be hidden at {secs}s; got {chip:?}"
            );
        }
    }

    #[test]
    fn footer_worked_chip_shows_humanized_label_above_threshold() {
        use std::time::Duration;
        // 1 minute on the dot — boundary, must render.
        let chip = super::footer_worked_chip(Duration::from_secs(60));
        let text: String = chip.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "worked 1m");

        // 3h 12m — the issue's golden example.
        let chip = super::footer_worked_chip(Duration::from_secs(11_550));
        let text: String = chip.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "worked 3h 12m");

        // Multi-day session — exercises the d/h band.
        let chip = super::footer_worked_chip(Duration::from_secs(2 * 86_400 + 5 * 3600));
        let text: String = chip.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "worked 2d 5h");
    }

    #[test]
    fn from_app_loading_state_uses_thinking_label_and_warning_color() {
        let app = make_app();
        let props = FooterProps::from_app(
            &app,
            None,
            "thinking \u{238B}",
            palette::STATUS_WARNING,
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
        );

        assert!(props.state_label.starts_with("thinking"));
        assert_eq!(props.state_color, palette::STATUS_WARNING);
    }

    #[test]
    fn from_app_statusline_colors_come_from_ui_theme() {
        let mut app = make_app();
        app.ui_theme.mode_agent = Color::Rgb(1, 2, 3);
        app.ui_theme.text_dim = Color::Rgb(4, 5, 6);
        app.ui_theme.text_hint = Color::Rgb(7, 8, 9);
        app.ui_theme.text_muted = Color::Rgb(10, 11, 12);
        app.ui_theme.footer_bg = Color::Rgb(13, 14, 15);

        let props = idle_props_for(&app);

        assert_eq!(props.mode_color, Color::Rgb(1, 2, 3));
        assert_eq!(props.text_dim_color, Color::Rgb(4, 5, 6));
        assert_eq!(props.text_hint_color, Color::Rgb(7, 8, 9));
        assert_eq!(props.text_muted_color, Color::Rgb(10, 11, 12));
        assert_eq!(props.footer_bg, Color::Rgb(13, 14, 15));
    }

    #[test]
    fn render_applies_footer_background_to_full_row() {
        let mut app = make_app();
        app.ui_theme.footer_bg = Color::Rgb(13, 14, 15);
        let props = idle_props_for(&app);
        let widget = FooterWidget::new(props);
        let area = ratatui::layout::Rect::new(0, 0, 60, 1);
        let mut buf = ratatui::buffer::Buffer::empty(area);

        widget.render(area, &mut buf);

        for x in 0..area.width {
            assert_eq!(buf[(x, 0)].bg, Color::Rgb(13, 14, 15));
        }
    }

    // ---- agents chip wording ----
    #[test]
    fn footer_agents_chip_is_empty_when_no_agents_running() {
        let chip = super::footer_agents_chip(0, Locale::En);
        assert!(chip.is_empty(), "0 agents in flight → no chip");
    }

    #[test]
    fn footer_agents_chip_uses_singular_for_one() {
        let chip = super::footer_agents_chip(1, Locale::En);
        assert_eq!(chip.len(), 1);
        assert_eq!(chip[0].content.as_ref(), "1 agent");
    }

    #[test]
    fn footer_agents_chip_uses_plural_for_many() {
        let chip = super::footer_agents_chip(3, Locale::En);
        assert_eq!(chip.len(), 1);
        assert_eq!(chip[0].content.as_ref(), "3 agents");
    }

    #[test]
    fn footer_agents_chip_renders_into_widget() {
        let app = make_app();
        let agents = super::footer_agents_chip(2, Locale::En);
        let props = FooterProps::from_app(
            &app,
            None,
            "ready",
            palette::TEXT_MUTED,
            Vec::<Span<'static>>::new(),
            agents,
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
        );
        let widget = FooterWidget::new(props);
        let area = ratatui::layout::Rect::new(0, 0, 60, 1);
        let mut buf = ratatui::buffer::Buffer::empty(area);
        widget.render(area, &mut buf);
        let rendered: String = (0..area.width).map(|x| buf[(x, 0)].symbol()).collect();
        assert!(
            rendered.contains("2 agents"),
            "expected agents chip in render: {rendered:?}",
        );
    }

    #[test]
    fn from_app_mode_color_matches_mode_for_each_variant() {
        let mut app = make_app();
        let cases = [
            (AppMode::Agent, "agent", palette::MODE_AGENT),
            (AppMode::Yolo, "yolo", palette::MODE_YOLO),
            (AppMode::Plan, "plan", palette::MODE_PLAN),
        ];
        for (mode, expected_label, expected_color) in cases {
            app.mode = mode;
            let props = idle_props_for(&app);
            assert_eq!(
                props.mode_label, expected_label,
                "label mismatch for {mode:?}",
            );
            assert_eq!(
                props.mode_color, expected_color,
                "color mismatch for {mode:?}",
            );
        }
    }

    #[test]
    fn footer_mcp_chip_hidden_when_no_servers() {
        assert!(super::footer_mcp_chip(None, 0).is_empty());
        assert!(super::footer_mcp_chip(Some(0), 0).is_empty());
    }

    #[test]
    fn footer_mcp_chip_shows_count_only_until_snapshot_arrives() {
        let spans = super::footer_mcp_chip(None, 3);
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "MCP 3");
    }

    #[test]
    fn footer_mcp_chip_uses_success_color_when_all_connected() {
        let spans = super::footer_mcp_chip(Some(3), 3);
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "MCP 3/3");
        assert_eq!(spans[0].style.fg, Some(palette::STATUS_SUCCESS));
    }

    #[test]
    fn footer_mcp_chip_uses_warning_color_when_partial() {
        let spans = super::footer_mcp_chip(Some(2), 3);
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "MCP 2/3");
        assert_eq!(spans[0].style.fg, Some(palette::STATUS_WARNING));
    }

    #[test]
    fn footer_mcp_chip_uses_error_color_when_zero_connected() {
        let spans = super::footer_mcp_chip(Some(0), 3);
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "MCP 0/3");
        assert_eq!(spans[0].style.fg, Some(palette::STATUS_ERROR));
    }

    #[test]
    fn render_shows_retry_banner_when_active() {
        // Since `FooterProps::retry` is now a captured snapshot rather
        // than a global read at render time, we can pin the state on
        // the props directly without touching the global surface.
        let app = make_app();
        let mut props = idle_props_for(&app);
        props.retry = crate::retry_status::RetryState::Active(crate::retry_status::RetryBanner {
            attempt: 2,
            deadline: std::time::Instant::now() + std::time::Duration::from_secs(7),
            reason: "rate limited".to_string(),
        });
        let widget = FooterWidget::new(props);
        let area = ratatui::layout::Rect::new(0, 0, 80, 1);
        let mut buf = ratatui::buffer::Buffer::empty(area);
        widget.render(area, &mut buf);
        let rendered: String = (0..area.width).map(|x| buf[(x, 0)].symbol()).collect();
        assert!(
            rendered.contains("retry 2"),
            "expected retry banner in render: {rendered:?}",
        );
        assert!(
            rendered.contains("rate limited"),
            "expected reason in render: {rendered:?}",
        );
    }

    #[test]
    fn render_shows_failure_row_when_failed() {
        let app = make_app();
        let mut props = idle_props_for(&app);
        props.retry = crate::retry_status::RetryState::Failed {
            reason: "upstream 500".to_string(),
            since: std::time::Instant::now(),
        };
        let widget = FooterWidget::new(props);
        let area = ratatui::layout::Rect::new(0, 0, 80, 1);
        let mut buf = ratatui::buffer::Buffer::empty(area);
        widget.render(area, &mut buf);
        let rendered: String = (0..area.width).map(|x| buf[(x, 0)].symbol()).collect();
        assert!(
            rendered.contains("failed"),
            "expected failure row: {rendered:?}",
        );
        assert!(
            rendered.contains("upstream 500"),
            "expected reason: {rendered:?}",
        );
    }

    #[test]
    fn left_spans_priority_retry_beats_toast_and_status() {
        let app = make_app();
        let mut props = idle_props_for(&app);
        props.state_label = "working".to_string();
        props.toast = Some(FooterToast {
            text: "saved".to_string(),
            color: palette::STATUS_SUCCESS,
        });
        props.retry = crate::retry_status::RetryState::Active(crate::retry_status::RetryBanner {
            attempt: 1,
            deadline: std::time::Instant::now() + std::time::Duration::from_secs(5),
            reason: "rate limited".to_string(),
        });
        let widget = FooterWidget::new(props);
        let area = ratatui::layout::Rect::new(0, 0, 120, 1);
        let mut buf = ratatui::buffer::Buffer::empty(area);
        widget.render(area, &mut buf);
        let rendered: String = (0..area.width).map(|x| buf[(x, 0)].symbol()).collect();
        assert!(
            rendered.contains("retry 1"),
            "retry must win over toast + status: {rendered:?}",
        );
        assert!(
            !rendered.contains("saved"),
            "toast must be hidden while retry active: {rendered:?}",
        );
    }

    #[test]
    fn left_spans_priority_toast_beats_status_when_no_retry() {
        let app = make_app();
        let mut props = idle_props_for(&app);
        props.state_label = "working".to_string();
        props.toast = Some(FooterToast {
            text: "saved".to_string(),
            color: palette::STATUS_SUCCESS,
        });
        let widget = FooterWidget::new(props);
        let area = ratatui::layout::Rect::new(0, 0, 120, 1);
        let mut buf = ratatui::buffer::Buffer::empty(area);
        widget.render(area, &mut buf);
        let rendered: String = (0..area.width).map(|x| buf[(x, 0)].symbol()).collect();
        assert!(
            rendered.contains("saved"),
            "toast must replace the status line: {rendered:?}",
        );
        assert!(
            !rendered.contains("working"),
            "status label must be hidden by toast: {rendered:?}",
        );
    }

    #[test]
    fn version_chip_renders_at_right() {
        let app = make_app();
        let props = idle_props_for(&app);
        let widget = FooterWidget::new(props);
        let area = ratatui::layout::Rect::new(0, 0, 120, 1);
        let mut buf = ratatui::buffer::Buffer::empty(area);
        widget.render(area, &mut buf);
        let rendered: String = (0..area.width).map(|x| buf[(x, 0)].symbol()).collect();
        let expected = format!("v{}", env!("CARGO_PKG_VERSION"));
        assert!(
            rendered.contains(&expected),
            "version chip must appear in footer: {rendered:?}",
        );
    }

    /// The footer no longer carries a context-utilisation chip — sidebar's
    /// Context panel already shows `context: T/W tokens (X%)` so the footer
    /// row stays empty even when `last_prompt_tokens` is known.
    #[test]
    fn ctx_chip_does_not_appear_in_footer_even_when_prompt_tokens_known() {
        let mut app = make_app();
        app.session.last_prompt_tokens = Some(20_000);
        let props = idle_props_for(&app);
        let widget = FooterWidget::new(props);
        let area = ratatui::layout::Rect::new(0, 0, 200, 1);
        let mut buf = ratatui::buffer::Buffer::empty(area);
        widget.render(area, &mut buf);
        let rendered: String = (0..area.width).map(|x| buf[(x, 0)].symbol()).collect();
        assert!(
            !rendered.contains("context left"),
            "ctx chip must NOT render in footer (sidebar owns it): {rendered:?}",
        );
        assert!(
            !rendered.contains('%'),
            "ctx percent must NOT appear in footer: {rendered:?}",
        );
    }

    #[test]
    fn render_emits_mode_and_model_when_idle() {
        let app = make_app();
        let props = idle_props_for(&app);
        let widget = FooterWidget::new(props);

        let area = ratatui::layout::Rect::new(0, 0, 60, 1);
        let mut buf = ratatui::buffer::Buffer::empty(area);
        widget.render(area, &mut buf);

        let rendered: String = (0..area.width).map(|x| buf[(x, 0)].symbol()).collect();
        assert!(rendered.contains("agent"));
        assert!(rendered.contains("deepseek-v4-flash"));
        assert!(!rendered.contains("ready"));
    }

    #[test]
    fn working_label_pulses_dots_through_full_cycle() {
        // The label sequence `working` → `working.` → `working..` →
        // `working...` then wraps back. Each frame is a discrete tick;
        // the cycle is exactly 4 frames so adjacent ticks visibly differ.
        assert_eq!(super::footer_working_label(0, Locale::En), "working");
        assert_eq!(super::footer_working_label(1, Locale::En), "working.");
        assert_eq!(super::footer_working_label(2, Locale::En), "working..");
        assert_eq!(super::footer_working_label(3, Locale::En), "working...");
        assert_eq!(
            super::footer_working_label(4, Locale::En),
            "working",
            "wraps back at frame 4",
        );
        assert_eq!(super::footer_working_label(7, Locale::En), "working...");
    }

    /// Render the footer at `width` and return the visible single-line text.
    fn render_at_width(props: FooterProps, width: u16) -> String {
        let area = ratatui::layout::Rect::new(0, 0, width, 1);
        let mut buf = ratatui::buffer::Buffer::empty(area);
        FooterWidget::new(props).render(area, &mut buf);
        (0..area.width)
            .map(|x| buf[(x, 0)].symbol())
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    fn props_with_status(state: &str) -> FooterProps {
        let app = make_app();
        FooterProps::from_app(
            &app,
            None,
            state,
            palette::DEEPSEEK_SKY,
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
        )
    }

    /// Issue #88 — at the widest tier the footer shows mode · model · status
    /// without any truncation.
    #[test]
    fn footer_priority_drop_full_at_120_cols() {
        let props = props_with_status("working");
        let line = render_at_width(props, 120);
        assert!(line.contains("agent"), "mode visible: {line:?}");
        assert!(
            line.contains("deepseek-v4-flash"),
            "model visible: {line:?}"
        );
        assert!(line.contains("working"), "status visible: {line:?}");
        assert!(!line.contains("..."), "no truncation expected: {line:?}");
    }

    #[test]
    fn footer_priority_drop_full_at_100_cols() {
        let props = props_with_status("working");
        let line = render_at_width(props, 100);
        assert!(line.contains("agent"));
        assert!(line.contains("deepseek-v4-flash"));
        assert!(line.contains("working"));
    }

    /// At 80 cols the short status label "working" still fits alongside mode +
    /// model. The line never wraps mid-hint.
    #[test]
    fn footer_priority_drop_full_at_80_cols() {
        let props = props_with_status("working");
        let line = render_at_width(props, 80);
        assert!(line.contains("agent"));
        assert!(line.contains("deepseek-v4-flash"));
        assert!(!line.contains("..."), "no mid-word truncation: {line:?}");
        assert!(
            line.chars().count() <= 80,
            "fits in 80 cols: {line:?}"
        );
    }

    /// Status drops before the model is truncated. With a longer status label
    /// at 40 cols the status segment is dropped to keep mode + model intact.
    #[test]
    fn footer_priority_drop_status_first_at_40_cols() {
        let props = props_with_status("refreshing context");
        // "agent · deepseek-v4-flash · refreshing context" = 46 cols. At 40
        // the status label drops, keeping mode + model verbatim.
        let line = render_at_width(props, 40);
        assert!(line.contains("agent"), "mode kept: {line:?}");
        assert!(
            line.contains("deepseek-v4-flash"),
            "model kept verbatim: {line:?}"
        );
        assert!(
            !line.contains("refreshing"),
            "status dropped before model truncated: {line:?}",
        );
        assert!(
            line.chars().count() <= 40,
            "fits in 40 cols: {line:?}"
        );
    }

    /// At 60 cols mode + model + a long status all just fit (49 cols), so the
    /// whole line is preserved.
    #[test]
    fn footer_priority_drop_full_at_60_cols() {
        let props = props_with_status("working");
        let line = render_at_width(props, 60);
        assert!(line.contains("agent"));
        assert!(line.contains("deepseek-v4-flash"));
        assert!(line.contains("working"));
    }

    /// Below 30 cols the model truncates with an ellipsis only after the
    /// status label has already been dropped. Mode label always survives.
    #[test]
    fn footer_priority_drop_truncates_model_only_when_status_already_gone() {
        let props = props_with_status("working");
        let line = render_at_width(props, 20);
        // Phase 5 follow-up: the whale brand glyph anchors the row. The
        // `mode` label sits right after it.
        assert!(
            line.starts_with("\u{1F433}"),
            "whale prefix is the row anchor: {line:?}"
        );
        assert!(line.contains("agent"), "mode stays after whale: {line:?}");
        assert!(
            line.contains("..."),
            "model truncated as last resort: {line:?}"
        );
        assert!(!line.contains("working"), "status dropped: {line:?}");
    }

    fn props_with_status_and_cost(state: &str, cost: &str) -> FooterProps {
        let app = make_app();
        FooterProps::from_app(
            &app,
            None,
            state,
            palette::DEEPSEEK_SKY,
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            vec![Span::styled(cost.to_string(), Style::default())],
            Vec::<Span<'static>>::new(),
        )
    }

    #[test]
    fn render_drops_oversized_right_chips_before_they_crowd_left_status() {
        let app = make_app();
        let long_cache = vec![Span::styled(
            "Cache: 75.0% hit | hit 36000 | miss 12000".to_string(),
            Style::default(),
        )];
        let props = FooterProps::from_app(
            &app,
            None,
            "ready",
            palette::TEXT_MUTED,
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            long_cache,
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
        );

        let line = render_at_width(props, 40);

        assert!(
            line.contains("agent"),
            "left status should survive: {line:?}"
        );
        assert!(
            !line.contains("Cache:"),
            "oversized right chip should drop instead of crowding the row: {line:?}",
        );
        assert!(line.width() <= 40, "footer must fit in one row: {line:?}");
    }

    /// Repro for the user-reported "right edge overflow with `Ctrl+O Activity:
    /// thinking` + ctx chip" bug. The status label, the cost+saved hint, and
    /// the ctx chip all stay alive at any reasonable width — but the painted
    /// row must never exceed `area.width`. The overflow guard needs to keep
    /// shrinking until *every* combination fits.
    #[test]
    fn render_does_not_overflow_with_long_status_and_ctx_chip() {
        let mut app = make_app();
        // Drive context_signal_spans to produce a non-trivial ctx chip.
        app.session.last_prompt_tokens = Some(690_000);
        let mut props = FooterProps::from_app(
            &app,
            None,
            "Ctrl+O Activity: thinking",
            palette::TEXT_MUTED,
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            vec![
                Span::styled("¥0.16".to_string(), Style::default()),
                Span::styled(
                    " · saved ¥1.98".to_string(),
                    Style::default().fg(palette::STATUS_SUCCESS),
                ),
            ],
            Vec::<Span<'static>>::new(),
        );
        props.retry = crate::retry_status::RetryState::Idle;

        for width in [60, 70, 80, 88, 100, 120, 140] {
            let line = render_at_width(props.clone(), width);
            assert!(
                line.width() <= width as usize,
                "footer at width={width} overflowed: rendered_width={} line={line:?}",
                line.width()
            );
        }
    }

    /// Tighter repro for the user-reported screenshot — the production state
    /// label tail is `Activity: thinking` (no `Ctrl+O` prefix on every locale)
    /// or `Activity: tool/<name>`. Combined with a long mode label, model,
    /// cost+saved hint, and the ctx chip, the row must still fit at any
    /// terminal width.
    #[test]
    fn render_does_not_overflow_with_max_realistic_left_and_full_right_cluster() {
        let mut app = make_app();
        app.session.last_prompt_tokens = Some(690_000);
        let mut props = FooterProps::from_app(
            &app,
            None,
            // Worst-case status seen in the screenshot: includes the
            // `Ctrl+O` affordance hint and an activity label that survives
            // the priority cascade. Together with the model + cost line
            // this is ~73 cols of left content.
            "Ctrl+O Activity: thinking",
            palette::TEXT_MUTED,
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            vec![
                Span::styled("¥0.16".to_string(), Style::default()),
                Span::styled(
                    " · saved ¥1.98".to_string(),
                    Style::default().fg(palette::STATUS_SUCCESS),
                ),
            ],
            Vec::<Span<'static>>::new(),
        );
        props.retry = crate::retry_status::RetryState::Idle;
        // Also exercise a non-empty `receipts` (turn token/cost summary)
        // since that's the chip that sits between worked and version in
        // the right cluster.
        props.receipts = vec![Span::styled(
            "+ 12.5k in / 3.2k out".to_string(),
            Style::default().fg(palette::STATUS_SUCCESS),
        )];

        // Sweep through every plausible single-row terminal width, including
        // the borderline 85–95 col range where the screenshot fell off.
        for width in 50u16..=160 {
            let line = render_at_width(props.clone(), width);
            assert!(
                line.width() <= width as usize,
                "footer at width={width} overflowed: rendered_width={} line={line:?}",
                line.width()
            );
        }
    }

    #[test]
    fn render_keeps_right_chips_when_left_status_leaves_room() {
        let app = make_app();
        let cache = vec![Span::styled(
            "Cache: 75.0% hit".to_string(),
            Style::default(),
        )];
        let props = FooterProps::from_app(
            &app,
            None,
            "ready",
            palette::TEXT_MUTED,
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            cache,
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
        );

        let line = render_at_width(props, 80);

        assert!(
            line.contains("agent"),
            "left status should render: {line:?}"
        );
        assert!(
            line.contains("Cache: 75.0% hit"),
            "right chip should render: {line:?}"
        );
        assert!(line.width() <= 80, "footer must fit in one row: {line:?}");
    }

    /// v0.6.6 redesign — cost lives on the LEFT, between model and status.
    /// At wide widths the line reads `mode · model · cost · status`.
    #[test]
    fn footer_cost_renders_in_left_cluster_at_wide_widths() {
        let props = props_with_status_and_cost("working", "$0.42");
        let line = render_at_width(props, 120);
        let mode_pos = line.find("agent").expect("mode visible");
        let model_pos = line.find("deepseek-v4-flash").expect("model visible");
        let cost_pos = line.find("$0.42").expect("cost visible on left");
        let status_pos = line.find("working").expect("status visible");
        assert!(mode_pos < model_pos);
        assert!(model_pos < cost_pos, "cost must follow model: {line:?}");
        assert!(cost_pos < status_pos, "cost must precede status: {line:?}");
    }

    /// Cost is preserved when status drops — cost is steady info, status is
    /// a transient signal.
    #[test]
    fn footer_cost_outranks_status_when_space_tight() {
        // "agent · deepseek-v4-flash · $0.42 · refreshing context" = 53 cols.
        // At 47 the status drops but the cost survives (47 ≥ 36 mode+model+cost).
        let props = props_with_status_and_cost("refreshing context", "$0.42");
        let line = render_at_width(props, 47);
        assert!(line.contains("agent"));
        assert!(line.contains("deepseek-v4-flash"));
        assert!(
            line.contains("$0.42"),
            "cost survives status drop: {line:?}"
        );
        assert!(!line.contains("refreshing"), "status dropped: {line:?}");
    }

    #[test]
    fn render_swaps_toast_for_status_line() {
        let app = make_app();
        let toast = super::FooterToast {
            text: "session saved".to_string(),
            color: Color::Green,
        };
        let props = FooterProps::from_app(
            &app,
            Some(toast),
            "ready",
            palette::TEXT_MUTED,
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
        );
        let widget = FooterWidget::new(props);

        let area = ratatui::layout::Rect::new(0, 0, 60, 1);
        let mut buf = ratatui::buffer::Buffer::empty(area);
        widget.render(area, &mut buf);

        let rendered: String = (0..area.width).map(|x| buf[(x, 0)].symbol()).collect();
        assert!(rendered.contains("session saved"));
        assert!(!rendered.contains("agent"));
        assert!(!rendered.contains("deepseek-v4-flash"));
    }

    #[test]
    fn render_clears_stale_cells_across_entire_footer_row() {
        let app = make_app();
        let widget = FooterWidget::new(idle_props_for(&app));
        let area = Rect::new(0, 0, 48, 1);
        let mut buf = Buffer::empty(area);

        for x in area.x..area.x.saturating_add(area.width) {
            buf[(x, area.y)]
                .set_symbol("X")
                .set_style(Style::default().fg(Color::Red).bg(Color::Blue));
        }

        widget.render(area, &mut buf);

        let rendered: String = (area.x..area.x.saturating_add(area.width))
            .map(|x| buf[(x, area.y)].symbol())
            .collect();

        assert!(
            !rendered.contains('X'),
            "footer render must clear stale row content before painting: {rendered:?}"
        );
        for x in area.x..area.x.saturating_add(area.width) {
            assert_eq!(
                buf[(x, area.y)].bg,
                app.ui_theme.footer_bg,
                "footer background should cover the full row"
            );
        }
    }
}
