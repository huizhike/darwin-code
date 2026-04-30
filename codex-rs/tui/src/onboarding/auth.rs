#![allow(clippy::unwrap_used)]

use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use crossterm::event::KeyModifiers;
use ratatui::buffer::Buffer;
use ratatui::layout::Constraint;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::prelude::Widget;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Borders;
use ratatui::widgets::Paragraph;
use ratatui::widgets::WidgetRef;
use ratatui::widgets::Wrap;
use std::cell::Cell;
use std::sync::Arc;
use std::sync::RwLock;

use super::onboarding_screen::StepState;
use crate::onboarding::onboarding_screen::KeyboardHandler;
use crate::onboarding::onboarding_screen::StepStateProvider;
use crate::tui::FrameRequester;

/// Marks buffer cells that have cyan+underlined style as an OSC 8 hyperlink.
///
/// Terminal emulators recognise the OSC 8 escape sequence and treat the entire
/// marked region as a single clickable link, regardless of row wrapping. This
/// is necessary because ratatui's cell-based rendering emits `MoveTo` at every
/// row boundary, which breaks normal terminal URL detection for long URLs that
/// wrap across multiple rows.
pub(crate) fn mark_url_hyperlink(buf: &mut Buffer, area: Rect, url: &str) {
    let safe_url: String = url
        .chars()
        .filter(|&c| c != '\x1B' && c != '\x07')
        .collect();
    if safe_url.is_empty() {
        return;
    }

    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let cell = &mut buf[(x, y)];
            if cell.fg != Color::Cyan || !cell.modifier.contains(Modifier::UNDERLINED) {
                continue;
            }
            let sym = cell.symbol().to_string();
            if sym.trim().is_empty() {
                continue;
            }
            cell.set_symbol(&format!("\x1B]8;;{safe_url}\x07{sym}\x1B]8;;\x07"));
        }
    }
}

#[derive(Clone)]
pub(crate) enum SignInState {
    PickMode,
    ApiKeyEntry(ApiKeyInputState),
    ApiKeyConfigured,
}

#[derive(Clone, Default)]
pub(crate) struct ApiKeyInputState {
    value: String,
    prepopulated_from_env: bool,
}

pub(crate) struct ByokSetupWidget {
    pub request_frame: FrameRequester,
    pub error: Arc<RwLock<Option<String>>>,
    pub sign_in_state: Arc<RwLock<SignInState>>,
    pub animations_suppressed: Cell<bool>,
}

impl KeyboardHandler for ByokSetupWidget {
    fn handle_key_event(&mut self, key_event: KeyEvent) {
        if self.handle_api_key_entry_key_event(&key_event) {
            return;
        }

        match key_event.code {
            KeyCode::Char('1') | KeyCode::Enter => {
                if matches!(&*self.sign_in_state.read().unwrap(), SignInState::PickMode) {
                    self.start_api_key_entry();
                }
            }
            KeyCode::Esc => self.cancel_active_attempt(),
            _ => {}
        }
    }

    fn handle_paste(&mut self, pasted: String) {
        let _ = self.handle_api_key_entry_paste(pasted);
    }
}

impl ByokSetupWidget {
    pub(crate) fn set_animations_suppressed(&self, suppressed: bool) {
        self.animations_suppressed.set(suppressed);
    }

    pub(crate) fn should_suppress_animations(&self) -> bool {
        false
    }

    pub(crate) fn cancel_active_attempt(&self) {
        *self.sign_in_state.write().unwrap() = SignInState::PickMode;
        self.set_error(None);
        self.request_frame.schedule_frame();
    }

    fn set_error(&self, message: Option<String>) {
        *self.error.write().unwrap() = message;
    }

    fn error_message(&self) -> Option<String> {
        self.error.read().unwrap().clone()
    }

    fn render_pick_mode(&self, area: Rect, buf: &mut Buffer) {
        let mut lines: Vec<Line> = vec![
            Line::from(vec!["> ".into(), "BYOK-only DarwinCode".bold()]),
            "".into(),
            "  DarwinCode no longer supports managed account login.".into(),
            "  Configure a standard provider API key through config.toml or an environment variable.".into(),
            "".into(),
            "  1. Provide your own API key".cyan().into(),
            "     Put the provider API key directly in config.toml as api_key".dim().into(),
            "".into(),
            "  Press Enter to continue".dim().into(),
        ];
        if let Some(error) = self.error_message() {
            lines.push("".into());
            lines.push(error.red().into());
        }
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }

    fn render_api_key_entry(&self, area: Rect, buf: &mut Buffer, state: &ApiKeyInputState) {
        let [intro_area, input_area, footer_area] = Layout::vertical([
            Constraint::Min(4),
            Constraint::Length(3),
            Constraint::Min(2),
        ])
        .areas(area);

        let mut intro_lines: Vec<Line> = vec![
            Line::from(vec!["> ".into(), "Configure BYOK API access".bold()]),
            "".into(),
            "  Pasted keys are not persisted by the TUI.".into(),
            "  Put the key directly in the selected provider block as api_key.".into(),
            "".into(),
        ];
        if state.prepopulated_from_env {
            intro_lines.push("  Detected OPENAI_API_KEY in the current environment.".into());
            intro_lines.push("".into());
        }
        Paragraph::new(intro_lines)
            .wrap(Wrap { trim: false })
            .render(intro_area, buf);

        let content_line: Line = if state.value.is_empty() {
            vec!["Paste or type your API key to validate local entry".dim()].into()
        } else {
            Line::from("•".repeat(state.value.chars().count().min(48)))
        };
        Paragraph::new(content_line)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title("API key")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .render(input_area, buf);

        let mut footer_lines: Vec<Line> = vec![
            "  Press Enter after exporting the key or updating config.toml"
                .dim()
                .into(),
            "  Press Esc to go back".dim().into(),
        ];
        if let Some(error) = self.error_message() {
            footer_lines.push("".into());
            footer_lines.push(error.red().into());
        }
        Paragraph::new(footer_lines)
            .wrap(Wrap { trim: false })
            .render(footer_area, buf);
    }

    fn render_api_key_configured(&self, area: Rect, buf: &mut Buffer) {
        Paragraph::new(vec![
            "✓ BYOK API key detected".green().into(),
            "".into(),
            "  DarwinCode will use the configured provider key from the environment/config.".into(),
        ])
        .wrap(Wrap { trim: false })
        .render(area, buf);
    }

    fn handle_api_key_entry_key_event(&mut self, key_event: &KeyEvent) -> bool {
        let mut should_finish = false;
        let mut should_request_frame = false;

        {
            let mut guard = self.sign_in_state.write().unwrap();
            if let SignInState::ApiKeyEntry(state) = &mut *guard {
                match key_event.code {
                    KeyCode::Esc => {
                        *guard = SignInState::PickMode;
                        self.set_error(None);
                        should_request_frame = true;
                    }
                    KeyCode::Enter => {
                        should_finish = true;
                    }
                    KeyCode::Backspace => {
                        if state.prepopulated_from_env {
                            state.value.clear();
                            state.prepopulated_from_env = false;
                        } else {
                            state.value.pop();
                        }
                        self.set_error(None);
                        should_request_frame = true;
                    }
                    KeyCode::Char(c)
                        if key_event.kind == KeyEventKind::Press
                            && !key_event.modifiers.contains(KeyModifiers::SUPER)
                            && !key_event.modifiers.contains(KeyModifiers::CONTROL)
                            && !key_event.modifiers.contains(KeyModifiers::ALT) =>
                    {
                        if state.prepopulated_from_env {
                            state.value.clear();
                            state.prepopulated_from_env = false;
                        }
                        state.value.push(c);
                        self.set_error(None);
                        should_request_frame = true;
                    }
                    _ => {}
                }
            } else {
                return false;
            }
        }

        if should_finish {
            self.finish_api_key_entry();
        } else if should_request_frame {
            self.request_frame.schedule_frame();
        }
        true
    }

    fn handle_api_key_entry_paste(&mut self, pasted: String) -> bool {
        let trimmed = pasted.trim();
        if trimmed.is_empty() {
            return false;
        }

        let mut guard = self.sign_in_state.write().unwrap();
        if let SignInState::ApiKeyEntry(state) = &mut *guard {
            if state.prepopulated_from_env {
                state.value = trimmed.to_string();
                state.prepopulated_from_env = false;
            } else {
                state.value.push_str(trimmed);
            }
            self.set_error(None);
        } else {
            return false;
        }

        drop(guard);
        self.request_frame.schedule_frame();
        true
    }

    fn start_api_key_entry(&mut self) {
        self.set_error(None);
        let prefill_from_env = std::env::var("OPENAI_API_KEY")
            .ok()
            .filter(|key| !key.is_empty());
        *self.sign_in_state.write().unwrap() = SignInState::ApiKeyEntry(ApiKeyInputState {
            value: prefill_from_env.clone().unwrap_or_default(),
            prepopulated_from_env: prefill_from_env.is_some(),
        });
        self.request_frame.schedule_frame();
    }

    fn finish_api_key_entry(&mut self) {
        let has_env_key = std::env::var("OPENAI_API_KEY")
            .ok()
            .is_some_and(|key| !key.is_empty());
        if has_env_key {
            self.set_error(None);
            *self.sign_in_state.write().unwrap() = SignInState::ApiKeyConfigured;
        } else {
            self.set_error(Some(
                "Set provider api_key in config.toml; TUI login storage is removed.".to_string(),
            ));
        }
        self.request_frame.schedule_frame();
    }
}

impl StepStateProvider for ByokSetupWidget {
    fn get_step_state(&self) -> StepState {
        match &*self.sign_in_state.read().unwrap() {
            SignInState::PickMode | SignInState::ApiKeyEntry(_) => StepState::InProgress,
            SignInState::ApiKeyConfigured => StepState::Complete,
        }
    }
}

impl WidgetRef for ByokSetupWidget {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        match &*self.sign_in_state.read().unwrap() {
            SignInState::PickMode => self.render_pick_mode(area, buf),
            SignInState::ApiKeyEntry(state) => self.render_api_key_entry(area, buf, state),
            SignInState::ApiKeyConfigured => self.render_api_key_configured(area, buf),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Modifier;
    use ratatui::style::Style;

    #[test]
    fn mark_url_hyperlink_wraps_cyan_underlined_cells() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 4, 1));
        let area = Rect::new(0, 0, 4, 1);
        for (idx, ch) in "link".chars().enumerate() {
            buf[(idx as u16, 0)].set_symbol(&ch.to_string()).set_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::UNDERLINED),
            );
        }

        mark_url_hyperlink(&mut buf, area, "https://example.com");

        assert!(
            buf[(0, 0)]
                .symbol()
                .starts_with("\x1B]8;;https://example.com")
        );
        assert!(buf[(3, 0)].symbol().ends_with("\x1B]8;;\x07"));
    }

    #[test]
    fn mark_url_hyperlink_sanitizes_control_chars() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 1, 1));
        let area = Rect::new(0, 0, 1, 1);
        buf[(0, 0)].set_symbol("x").set_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::UNDERLINED),
        );

        mark_url_hyperlink(&mut buf, area, "https://ex\x1bample.com\x07/path");

        let symbol = buf[(0, 0)].symbol();
        assert!(symbol.contains("https://example.com/path"));
        assert!(!symbol.contains("https://ex\x1bample"));
    }
}
