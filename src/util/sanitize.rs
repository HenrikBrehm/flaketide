//! Sanitize untrusted text before display or persistence.
//!
//! Test stdout/stderr is captured verbatim and ends up in the SQLite store
//! and the TUI. If a test prints terminal escape codes (ANSI colors, cursor
//! movement, OSC 8 hyperlinks, alternate-screen toggles) those can corrupt
//! or hijack the user's terminal when the data is rendered. We strip them
//! at sanitize time.
//!
//! Strips: C0 control bytes (0x00-0x1f) except `\t` and `\n`; the DEL byte
//! (0x7f); CSI/OSC ESC sequences; alternate-charset switches.

pub fn sanitize_terminal_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\u{1b}' => skip_escape_sequence(&mut chars),
            '\t' | '\n' => out.push(c),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {}
            c => out.push(c),
        }
    }
    out
}

fn skip_escape_sequence(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    let Some(&next) = chars.peek() else { return };
    match next {
        '[' => {
            chars.next();
            while let Some(&c) = chars.peek() {
                chars.next();
                if matches!(c, '@'..='~') {
                    break;
                }
            }
        }
        ']' => {
            chars.next();
            while let Some(&c) = chars.peek() {
                chars.next();
                if c == '\u{07}' {
                    break;
                }
                if c == '\u{1b}' {
                    if let Some(&'\\') = chars.peek() {
                        chars.next();
                    }
                    break;
                }
            }
        }
        '(' | ')' | '*' | '+' => {
            chars.next();
            chars.next();
        }
        _ => {
            chars.next();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_plain_text() {
        assert_eq!(sanitize_terminal_text("hello world"), "hello world");
    }

    #[test]
    fn keeps_tab_and_newline() {
        assert_eq!(sanitize_terminal_text("a\tb\nc"), "a\tb\nc");
    }

    #[test]
    fn strips_csi_color() {
        assert_eq!(sanitize_terminal_text("\u{1b}[31mred\u{1b}[0m"), "red");
    }

    #[test]
    fn strips_clear_screen() {
        assert_eq!(sanitize_terminal_text("a\u{1b}[2Jb"), "ab");
    }

    #[test]
    fn strips_osc_title() {
        assert_eq!(sanitize_terminal_text("\u{1b}]0;EVIL\u{07}safe"), "safe");
    }

    #[test]
    fn strips_osc_hyperlink() {
        let input = "\u{1b}]8;;file:///etc/passwd\u{07}click\u{1b}]8;;\u{07}";
        assert_eq!(sanitize_terminal_text(input), "click");
    }

    #[test]
    fn strips_alternate_screen_toggle() {
        assert_eq!(sanitize_terminal_text("a\u{1b}[?1049hb"), "ab");
    }

    #[test]
    fn strips_del_and_bell() {
        assert_eq!(sanitize_terminal_text("a\u{7f}b\u{07}c"), "abc");
    }
}
