//! Native terminal presentation. A VT row remains one row regardless of color
//! changes; empty cells and wide-character continuation cells preserve columns.
use gpui::{
    FontStyle, FontWeight, HighlightStyle, Rgba, SharedString, StyledText, UnderlineStyle, px, rgb,
};
use gpui_kit_material::prelude::{ColorRole, color};

#[derive(Debug)]
pub struct TerminalRow {
    pub text: String,
    pub highlights: Vec<(std::ops::Range<usize>, HighlightStyle)>,
}
impl TerminalRow {
    pub fn element(self) -> StyledText {
        StyledText::new(SharedString::from(self.text)).with_highlights(self.highlights)
    }
}

pub fn terminal_rows(screen: &vt100::Screen, show_cursor: bool) -> Vec<TerminalRow> {
    let (rows, columns) = screen.size();
    let cursor = screen.cursor_position();
    (0..rows)
        .map(|row| {
            let mut text = String::new();
            let mut highlights = Vec::new();
            let mut run: Option<(usize, HighlightStyle)> = None;
            for column in 0..columns {
                let Some(cell) = screen.cell(row, column) else {
                    continue;
                };
                if cell.is_wide_continuation() {
                    continue;
                }
                let mut foreground = terminal_color(cell.fgcolor(), false);
                let mut background = terminal_color(cell.bgcolor(), true);
                if cell.inverse() {
                    std::mem::swap(&mut foreground, &mut background);
                }
                if show_cursor
                    && !screen.hide_cursor()
                    && screen.scrollback() == 0
                    && cursor == (row, column)
                {
                    foreground = color(ColorRole::ON_PRIMARY);
                    background = color(ColorRole::PRIMARY);
                }
                let style = HighlightStyle {
                    color: Some(foreground.into()),
                    background_color: Some(background.into()),
                    font_weight: Some(if cell.bold() {
                        FontWeight::BOLD
                    } else {
                        FontWeight::NORMAL
                    }),
                    font_style: Some(if cell.italic() {
                        FontStyle::Italic
                    } else {
                        FontStyle::Normal
                    }),
                    underline: cell.underline().then(|| UnderlineStyle {
                        thickness: px(1.),
                        color: Some(foreground.into()),
                        wavy: false,
                    }),
                    ..Default::default()
                };
                if let Some((start, previous)) = run
                    && previous != style
                {
                    highlights.push((start..text.len(), previous));
                    run = Some((text.len(), style));
                } else if run.is_none() {
                    run = Some((text.len(), style));
                }
                text.push_str(if cell.contents().is_empty() {
                    " "
                } else {
                    cell.contents()
                });
            }
            if let Some((start, style)) = run {
                highlights.push((start..text.len(), style));
            }
            TerminalRow { text, highlights }
        })
        .collect()
}

/// Escape-sequence colors are remote data, not application theme constants.
/// Default colors follow the theme; truecolor and xterm's 256-color cube retain
/// the values requested by the remote program.
fn terminal_color(value: vt100::Color, background: bool) -> Rgba {
    match value {
        vt100::Color::Default => color(if background {
            ColorRole::INVERSE_SURFACE
        } else {
            ColorRole::INVERSE_ON_SURFACE
        }),
        vt100::Color::Rgb(r, g, b) => {
            rgb((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b))
        }
        vt100::Color::Idx(index) => {
            let ansi = [
                0x16161d, 0xff7b72, 0x5fd383, 0xe8c26a, 0x7cb7ff, 0xd2a8ff, 0x76e3ea, 0xe6e7ea,
                0x8b8e96, 0xffa198, 0x9be9a8, 0xf5df9c, 0xa5d6ff, 0xe2c5ff, 0xb3f0f4, 0xffffff,
            ];
            if index < 16 {
                return rgb(ansi[index as usize]);
            }
            if index >= 232 {
                let gray = u32::from(index - 232) * 10 + 8;
                return rgb((gray << 16) | (gray << 8) | gray);
            }
            let index = u32::from(index - 16);
            let component = |value| if value == 0 { 0 } else { value * 40 + 55 };
            rgb((component(index / 36) << 16)
                | (component(index / 6 % 6) << 8)
                | component(index % 6))
        }
    }
}

/// Ctrl/Alt and xterm navigation keys, including application cursor mode.
pub fn key_bytes(event: &gpui::KeyDownEvent, application_cursor: bool) -> Option<Vec<u8>> {
    let key = event.keystroke.key.as_str();
    let modifiers = event.keystroke.modifiers;
    if modifiers.platform {
        return None;
    }
    let modifier = 1
        + u8::from(modifiers.shift)
        + 2 * u8::from(modifiers.alt)
        + 4 * u8::from(modifiers.control);
    if modifier > 1 {
        let final_byte = match key {
            "up" => Some('A'),
            "down" => Some('B'),
            "right" => Some('C'),
            "left" => Some('D'),
            "home" => Some('H'),
            "end" => Some('F'),
            _ => None,
        };
        if let Some(final_byte) = final_byte {
            return Some(format!("\u{1b}[1;{modifier}{final_byte}").into_bytes());
        }
    }
    let numbered = match key {
        "insert" => Some(2),
        "delete" => Some(3),
        "pageup" => Some(5),
        "pagedown" => Some(6),
        _ => None,
    };
    if let Some(number) = numbered {
        return Some(
            if modifier == 1 {
                format!("\u{1b}[{number}~")
            } else {
                format!("\u{1b}[{number};{modifier}~")
            }
            .into_bytes(),
        );
    }
    if let Some(number) = key
        .strip_prefix('f')
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|number| (1..=20).contains(number))
    {
        if number <= 4 {
            let final_byte = char::from(b'P' + number as u8 - 1);
            return Some(
                if modifier == 1 {
                    format!("\u{1b}O{final_byte}")
                } else {
                    format!("\u{1b}[1;{modifier}{final_byte}")
                }
                .into_bytes(),
            );
        }
        let code = [
            15, 17, 18, 19, 20, 21, 23, 24, 25, 26, 28, 29, 31, 32, 33, 34,
        ][number - 5];
        return Some(
            if modifier == 1 {
                format!("\u{1b}[{code}~")
            } else {
                format!("\u{1b}[{code};{modifier}~")
            }
            .into_bytes(),
        );
    }
    let control = match key {
        "space" | " " | "2" => Some(0),
        "3" => Some(27),
        "4" => Some(28),
        "5" => Some(29),
        "6" => Some(30),
        "7" | "/" | "-" => Some(31),
        "8" | "?" => Some(127),
        _ => None,
    };
    let mut bytes = if let Some(control) = control.filter(|_| modifiers.control) {
        vec![control]
    } else if modifiers.control && key.len() == 1 {
        let character = key.as_bytes()[0].to_ascii_uppercase();
        if (b'@'..=b'_').contains(&character) {
            vec![character & 0x1f]
        } else {
            return None;
        }
    } else {
        match key {
            "enter" => vec![b'\r'],
            "backspace" => vec![0x7f],
            "tab" if modifiers.shift => b"\x1b[Z".to_vec(),
            "tab" => vec![b'\t'],
            "escape" => vec![0x1b],
            "up" => if application_cursor {
                b"\x1bOA"
            } else {
                b"\x1b[A"
            }
            .to_vec(),
            "down" => if application_cursor {
                b"\x1bOB"
            } else {
                b"\x1b[B"
            }
            .to_vec(),
            "right" => if application_cursor {
                b"\x1bOC"
            } else {
                b"\x1b[C"
            }
            .to_vec(),
            "left" => if application_cursor {
                b"\x1bOD"
            } else {
                b"\x1b[D"
            }
            .to_vec(),
            "home" => b"\x1b[H".to_vec(),
            "end" => b"\x1b[F".to_vec(),
            _ => event.keystroke.key_char.as_ref()?.as_bytes().to_vec(),
        }
    };
    if modifiers.alt {
        bytes.insert(0, 0x1b);
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn colors_do_not_split_rows_or_remove_spaces() {
        let mut parser = vt100::Parser::new(3, 20, 10);
        parser.process(b"\x1b[31mred\x1b[0m  normal\r\nsecond");
        let rows = terminal_rows(parser.screen(), false);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].text, "red  normal         ");
        assert!(rows[0].highlights.len() >= 2);
        assert_eq!(rows[1].text.trim_end(), "second");
    }
    #[test]
    fn wide_characters_do_not_add_an_extra_column() {
        let mut parser = vt100::Parser::new(2, 8, 0);
        parser.process("中 a".as_bytes());
        let rows = terminal_rows(parser.screen(), false);
        assert_eq!(rows[0].text, "中 a    ");
    }
    #[test]
    fn control_key_and_application_arrows() {
        let event = |text| gpui::KeyDownEvent {
            keystroke: gpui::Keystroke::parse(text).unwrap(),
            is_held: false,
            prefer_character_input: false,
        };
        assert_eq!(key_bytes(&event("ctrl-c"), false), Some(vec![3]));
        assert_eq!(key_bytes(&event("ctrl-a"), false), Some(vec![1]));
        assert_eq!(key_bytes(&event("up"), true), Some(b"\x1bOA".to_vec()));
        assert_eq!(key_bytes(&event("up"), false), Some(b"\x1b[A".to_vec()));
        assert_eq!(
            key_bytes(&event("ctrl-left"), false),
            Some(b"\x1b[1;5D".to_vec())
        );
        assert_eq!(
            key_bytes(&event("alt-delete"), false),
            Some(b"\x1b[3;3~".to_vec())
        );
        assert_eq!(key_bytes(&event("f12"), false), Some(b"\x1b[24~".to_vec()));
        assert_eq!(
            key_bytes(&event("shift-f1"), false),
            Some(b"\x1b[1;2P".to_vec())
        );
        assert_eq!(key_bytes(&event("ctrl-space"), false), Some(vec![0]));
    }
}
