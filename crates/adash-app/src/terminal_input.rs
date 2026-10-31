//! Native text services for the PTY. Composition stays local until committed;
//! each input entity owns the sender of exactly one terminal session.
use gpui::{
    Bounds, Context, EntityInputHandler, FocusHandle, Pixels, Point, UTF16Selection, Window,
};
use std::{ops::Range, sync::mpsc::Sender};

pub struct TerminalInput {
    pub focus: FocusHandle,
    pub sender: Option<Sender<adash_ssh::TerminalCommand>>,
    pub enabled: bool,
    pub preedit: String,
    pub bounds: Bounds<Pixels>,
    selection: Range<usize>,
}
impl TerminalInput {
    pub fn reset(&mut self) {
        self.sender = None;
        self.enabled = false;
        self.preedit.clear();
        self.selection = 0..0;
    }
    pub fn new(focus: FocusHandle) -> Self {
        Self {
            focus,
            sender: None,
            enabled: false,
            preedit: String::new(),
            bounds: Bounds::default(),
            selection: 0..0,
        }
    }
    fn send(&self, text: &str) {
        if self.enabled
            && let Some(sender) = &self.sender
        {
            let _ = sender.send(adash_ssh::TerminalCommand::Input(text.as_bytes().to_vec()));
        }
    }
}
fn utf16_slice(text: &str, range: Range<usize>) -> (String, Range<usize>) {
    let units: Vec<_> = text.encode_utf16().collect();
    let mut start = range.start.min(units.len());
    let mut end = range.end.max(start).min(units.len());
    if start < units.len() && start > 0 && (0xdc00..=0xdfff).contains(&units[start]) {
        start -= 1;
    }
    if end < units.len() && (0xdc00..=0xdfff).contains(&units[end]) {
        end += 1;
    }
    (String::from_utf16_lossy(&units[start..end]), start..end)
}
impl EntityInputHandler for TerminalInput {
    fn accepts_text_input(&self, window: &mut Window, _: &mut Context<Self>) -> bool {
        self.enabled && self.sender.is_some() && self.focus.is_focused(window)
    }
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let (text, range) = utf16_slice(&self.preedit, range);
        *adjusted = Some(range);
        Some(text)
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        self.enabled.then(|| UTF16Selection {
            range: self.selection.clone(),
            reversed: false,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        (!self.preedit.is_empty()).then(|| 0..self.preedit.encode_utf16().count())
    }
    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = std::mem::take(&mut self.preedit);
        self.selection = 0..0;
        if self.accepts_text_input(window, cx) {
            self.send(&text);
        }
        window.notify_text_input_changed(&self.focus);
        cx.notify();
    }
    fn replace_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preedit.clear();
        self.selection = 0..0;
        if self.accepts_text_input(window, cx) {
            self.send(text);
        }
        window.notify_text_input_changed(&self.focus);
        cx.notify();
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.accepts_text_input(window, cx) {
            return;
        }
        self.preedit = text.into();
        let length = text.encode_utf16().count();
        self.selection = selected.unwrap_or(length..length);
        window.notify_text_input_changed(&self.focus);
        cx.notify();
    }
    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        Some(self.bounds)
    }
    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.preedit.encode_utf16().count())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext, InteractiveElement, IntoElement, Render, Styled, TestAppContext, div};
    impl Render for TerminalInput {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().track_focus(&self.focus)
        }
    }
    #[gpui::test]
    fn ime_is_local_until_commit_and_cannot_write_to_an_unfocused_session(cx: &mut TestAppContext) {
        let (sender, receiver) = std::sync::mpsc::channel();
        let window = cx.add_window(|_, cx| {
            let mut input = TerminalInput::new(cx.focus_handle());
            input.enabled = true;
            input.sender = Some(sender);
            input
        });
        let input = window.root(cx).unwrap();
        cx.update_window(window.into(), |_, window, cx| {
            window.draw(cx).clear(cx);
            input.read(cx).focus.clone().focus(window, cx);
            input.update(cx, |input, cx| {
                input.replace_and_mark_text_in_range(None, "zhong", None, window, cx);
                assert!(
                    receiver.try_recv().is_err(),
                    "preedit must never reach a remote shell"
                );
                input.replace_and_mark_text_in_range(None, "中文😀", None, window, cx);
                input.replace_text_in_range(None, "中文😀", window, cx);
                let adash_ssh::TerminalCommand::Input(bytes) = receiver.try_recv().unwrap() else {
                    panic!("expected committed text");
                };
                assert_eq!(bytes, "中文😀".as_bytes());
                assert!(
                    receiver.try_recv().is_err(),
                    "a commit must be sent only once"
                );
                window.blur(cx);
                input.replace_text_in_range(None, "wrong session", window, cx);
                assert!(receiver.try_recv().is_err());
                input.focus.clone().focus(window, cx);
                input.enabled = false;
                input.replace_text_in_range(None, "behind modal", window, cx);
                assert!(receiver.try_recv().is_err());
            });
        })
        .unwrap();
    }
    #[test]
    fn native_ime_ranges_preserve_surrogate_pairs() {
        assert_eq!(utf16_slice("中😀文", 2..3), ("😀".into(), 1..3));
        assert_eq!(utf16_slice("中😀文", 99..100), (String::new(), 4..4));
    }
}
