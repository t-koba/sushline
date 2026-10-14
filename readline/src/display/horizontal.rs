//! Stateful, cell-based single-line redisplay. The prompt and buffer share a
//! viewport; indicators replace cells, rather than being added to its width.

use crate::width::char_width;
use std::ops::Range;

#[derive(Debug, Default)]
pub(crate) struct HorizontalViewport {
    margin: usize,
    visible: Range<usize>,
}

#[derive(Debug, Default)]
pub(crate) struct DisplayLine {
    units: Vec<Unit>,
    prefix: String,
    pub(crate) width: usize,
    pub(crate) point: usize,
}

#[derive(Debug)]
struct Unit {
    text: String,
    width: usize,
    hidden: bool,
}

impl DisplayLine {
    /// An indivisible rendered character (including a control/meta expansion).
    pub(crate) fn push_cell(&mut self, text: String, width: usize) {
        // Keep zero-width scalars distinct from hidden bytes. Their clipping
        // follows the preceding base, even across intervening hidden units;
        // merging into an escape would replay a mark without its clipped base.
        self.units.push(Unit {
            text,
            width,
            hidden: false,
        });
        self.width += width;
    }

    pub(crate) fn push_hidden(&mut self, text: String) {
        if text.contains('\n') {
            self.finish_prompt_line(&text);
        } else {
            self.units.push(Unit {
                text,
                width: 0,
                hidden: true,
            });
        }
    }

    /// Prompt text: keep complete ANSI sequences as zero-cell units. Explicit
    /// Readline hidden spans are supplied separately by the prompt parser.
    pub(crate) fn push_prompt_text(&mut self, text: &str) {
        let mut chars = text.char_indices().peekable();
        while let Some((start, ch)) = chars.next() {
            if ch == '\n' {
                self.finish_prompt_line("\n");
            } else if ch == '\x1b' {
                match chars.next().map(|(_, ch)| ch) {
                    Some('[') => {
                        for (_, ch) in chars.by_ref() {
                            if ('@'..='~').contains(&ch) {
                                break;
                            }
                        }
                    }
                    Some(']') => {
                        let mut previous = '\0';
                        for (_, ch) in chars.by_ref() {
                            if ch == '\x07' || (previous == '\x1b' && ch == '\\') {
                                break;
                            }
                            previous = ch;
                        }
                    }
                    _ => {}
                }
                let end = chars.peek().map_or(text.len(), |(idx, _)| *idx);
                self.push_hidden(text[start..end].to_string());
            } else {
                self.push_cell(ch.to_string(), char_width(ch));
            }
        }
    }

    fn finish_prompt_line(&mut self, suffix: &str) {
        for unit in self.units.drain(..) {
            self.prefix.push_str(&unit.text);
        }
        self.prefix.push_str(suffix);
        self.width = 0;
    }

    pub(crate) fn append(&mut self, other: Self) {
        if !other.prefix.is_empty() {
            self.finish_prompt_line(&other.prefix);
        }
        self.point = self.width + other.point;
        self.width += other.width;
        self.units.extend(other.units);
    }
}

impl HorizontalViewport {
    pub(crate) fn layout(&mut self, line: &DisplayLine, columns: usize) -> (String, usize) {
        // Leave the final terminal cell unused: neither output nor cursor
        // placement should trigger automatic wrapping in horizontal mode.
        let capacity = columns.saturating_sub(1);
        if capacity == 0 {
            self.margin = 0;
            self.visible = 0..0;
            return (line.prefix.clone(), 0);
        }
        let edge = columns.saturating_sub(2).max(1);
        let third = (edge / 3).max(1);
        // Bash 5.3 ASCII PTY checkpoints activate at columns-2, but restore
        // the full line after shrinking to columns-4. Keep that hysteresis:
        // merely fitting the row, or moving point below the activation edge,
        // must not discard a scrolled window. Scroll roughly a third of the
        // screen; a large jump (or batch) differs from repeated small steps.
        if line.width <= columns.saturating_sub(4) {
            self.margin = 0;
        } else if line.point <= self.margin + third {
            // At 80 columns, the normalized margin is 50: Bash keeps it at
            // logical points 79/78/77, but restores the prompt at point 76.
            self.margin = if line.point < edge {
                0
            } else {
                line.point.saturating_sub(third)
            };
        } else if line.point - self.margin >= edge {
            self.margin = line.point.saturating_sub((2 * third).min(capacity));
        }
        let left = usize::from(self.margin > 0);
        let right = usize::from(line.width > self.margin + capacity && capacity > left);
        let start = self.margin + left;
        let end = self.margin + capacity - right;
        self.visible = start..end.min(line.width);
        let mut out = line.prefix.clone();
        if left > 0 {
            out.push('<');
        }
        let mut col = 0;
        let mut base_visible = false;
        for unit in &line.units {
            let next = col + unit.width;
            if unit.width == 0 {
                // Replay style/hidden bytes even outside the window, including
                // resets after clipped text. Never cut an escape sequence.
                if unit.hidden || base_visible {
                    out.push_str(&unit.text);
                }
            } else if col >= start && next <= end {
                out.push_str(&unit.text);
                base_visible = true;
            } else {
                base_visible = false;
                // A boundary through a wide glyph/expansion gets blank cells,
                // not a sliced UTF-8 string, caret pair, or octal escape.
                let overlap = next.min(end).saturating_sub(col.max(start));
                out.extend(std::iter::repeat_n(' ', overlap));
            }
            col = next;
        }
        if right > 0 {
            out.push('>');
        }
        (out, line.point.saturating_sub(self.margin).min(capacity))
    }

    /// Map a one-cell logical target in the last layout, excluding clipped
    /// cells and indicators. An invisible target is not clamped to another cell.
    pub(crate) fn visible_column(&self, position: usize) -> Option<usize> {
        self.visible
            .contains(&position)
            .then(|| position - self.margin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::{LineBuffer, RenderOptions};
    use crate::prompt::Prompt;
    use crate::width::visible_width;

    fn line(prompt: &str, text: &str, point: usize) -> DisplayLine {
        let prompt = Prompt::new(prompt);
        let mut line = prompt.horizontal_line();
        let mut buffer = LineBuffer::from(text);
        buffer.set_point(point);
        line.append(buffer.horizontal_line(None, RenderOptions::default(), prompt.width()));
        line
    }

    #[test]
    fn horizontal_scroll_ascii_edges_and_return_to_prompt() {
        let mut viewport = HorizontalViewport::default();
        for len in 0..=90 {
            let text = "a".repeat(len);
            let result = viewport.layout(&line("SUSHLINE_READY>", &text, len), 80);
            if len == 90 {
                assert_eq!(result, (format!("<{}", "a".repeat(52)), 53));
            }
        }
        assert_eq!(
            viewport.layout(&line("SUSHLINE_READY>", &"a".repeat(90), 0), 80),
            (format!("SUSHLINE_READY>{}>", "a".repeat(63)), 15)
        );
        let text = "a".repeat(30);
        let mut viewport = HorizontalViewport::default();
        assert_eq!(
            viewport.layout(&line("P>", &text, 0), 12),
            (format!("P>{}>", "a".repeat(8)), 2)
        );
        assert_eq!(
            viewport.layout(&line("P>", &text, 8), 12),
            (format!("<{}>", "a".repeat(9)), 6)
        );
        // A small move left stays within the current viewport.
        assert_eq!(viewport.layout(&line("P>", &text, 7), 12).1, 5);
        assert_eq!(viewport.layout(&line("P>", &text, 0), 12).1, 2);
    }

    fn scrolled_digits() -> (HorizontalViewport, String) {
        let text = "0123456789".repeat(10);
        let mut viewport = HorizontalViewport::default();
        for len in 0..=text.len() {
            viewport.layout(&line("P>", &text[..len], len), 80);
        }
        assert!(viewport.margin > 0);
        (viewport, text)
    }

    #[test]
    fn horizontal_scroll_activates_before_the_line_overflows() {
        let mut viewport = HorizontalViewport::default();
        for len in 0..=77 {
            let text = "a".repeat(len);
            let layout = viewport.layout(&line("P>", &text, len), 80);
            match len + 2 {
                77 => assert_eq!(layout, (format!("P>{text}"), 77)),
                78 => assert_eq!(layout, (format!("<{}", "a".repeat(51)), 52)),
                79 => assert_eq!(layout, (format!("<{}", "a".repeat(52)), 53)),
                _ => {}
            }
        }
    }

    #[test]
    fn horizontal_scroll_restores_prompt_only_after_the_line_shrinks_past_hysteresis() {
        let (mut viewport, text) = scrolled_digits();
        viewport.layout(&line("P>", &text, text.len()), 80); // Ctrl-E
        for len in (20..text.len()).rev() {
            let layout = viewport.layout(&line("P>", &text[..len], len), 80);
            if len >= 75 {
                // Widths 79, 78 and 77 still retain the bytewise end window.
                assert_eq!(viewport.margin, 26);
                assert_eq!(layout, (format!("<{}", &text[25..len]), len + 2 - 26));
            } else {
                // Width 76 restores the full prompt, as do shorter lines.
                assert_eq!(viewport.margin, 0);
                assert_eq!(layout, (format!("P>{}", &text[..len]), len + 2));
            }
        }
    }

    #[test]
    fn horizontal_scroll_keeps_long_line_history_when_point_moves_below_the_edge() {
        for bytewise in [true, false] {
            let (mut viewport, text) = if bytewise {
                scrolled_digits()
            } else {
                (HorizontalViewport::default(), "0123456789".repeat(10))
            };
            let mut layout = line("P>", &text, text.len());
            viewport.layout(&layout, 80);
            // Bytewise typing gives margin 26; a single end jump gives 50.
            let margin = if bytewise { 26 } else { 50 };
            assert_eq!(viewport.margin, margin);
            for point in [77, 76, 75] {
                layout = line("P>", &text, point); // prompt adds two cells
                assert_eq!(
                    viewport.layout(&layout, 80),
                    (format!("<{}", &text[margin - 1..]), point + 2 - margin)
                );
                assert_eq!(viewport.margin, margin);
            }
            // Restore at buffer point 50 for bytewise typing, 74 for an end
            // jump. Both trajectories show the full prompt at point 50.
            for point in [margin + 24, 50] {
                layout = line("P>", &text, point);
                assert_eq!(
                    viewport.layout(&layout, 80),
                    (format!("P>{}>", &text[..76]), point + 2)
                );
                assert_eq!(viewport.margin, 0);
            }
        }
    }

    #[test]
    fn horizontal_scroll_restores_prompt_after_widening_past_hysteresis() {
        let (mut viewport, text) = scrolled_digits();
        let layout = line("P>", &text, text.len());
        let margin = viewport.margin;
        viewport.layout(&layout, 90);
        assert_eq!(viewport.margin, margin); // preserve history while overlong
        assert_eq!(viewport.layout(&layout, 120), (format!("P>{text}"), 102));
        assert_eq!(viewport.margin, 0);
        // Narrowing recomputes a window; an exact fit still retains it. The
        // columns-4 rule at other widths is a Sushline-only regression here.
        let scrolled = viewport.layout(&layout, 80);
        assert!(scrolled.0.starts_with('<'));
        assert_eq!(viewport.layout(&layout, 103), scrolled);
        assert_eq!(viewport.layout(&layout, 106), (format!("P>{text}"), 102));
        assert_eq!(viewport.margin, 0);
    }

    #[test]
    fn horizontal_scroll_combining_marks_follow_the_base_across_hidden_units() {
        for prompt in [
            "e\x1b[31m\u{301}\x1b[1m\u{302}",
            "e\x01\x1b[31m\x02\u{301}\x01\x1b[1m\x02\u{302}",
        ] {
            let mut viewport = HorizontalViewport::default();
            let mut layout = line(prompt, &"x".repeat(20), 0);
            layout.push_hidden("\x1b[0m".into());
            assert_eq!(
                viewport.layout(&layout, 8),
                ("e\x1b[31m\u{301}\x1b[1m\u{302}xxxxx\x1b[0m>".into(), 1)
            );
            layout.point = layout.width;
            let (out, col) = viewport.layout(&layout, 8);
            // Hidden styles still replay, in order, but neither orphaned mark
            // can decorate the left indicator after its base is clipped.
            assert_eq!(out, "<\x1b[31m\x1b[1mxxx\x1b[0m");
            assert_eq!(col, 4);
        }
    }

    #[test]
    fn horizontal_scroll_maps_only_visible_cells_not_indicators() {
        let mut viewport = HorizontalViewport::default();
        let layout = line("P>", &"a".repeat(30), 8);
        viewport.layout(&layout, 12);
        assert_eq!(viewport.visible_column(5), Some(1));
        assert_eq!(viewport.visible_column(13), Some(9));
        for position in [0, 4, 14, 32] {
            assert_eq!(viewport.visible_column(position), None);
        }
        viewport.layout(&layout, 1);
        assert_eq!(viewport.visible_column(5), None);
    }

    #[test]
    fn horizontal_scroll_preserves_prompt_escapes_and_hidden_widths() {
        for prompt in [
            "\x01\x1b[31m\x02界>\x01\x1b[0m\x02",
            "\\[\\e[31m\\]界>\\[\\e[0m\\]",
            "\x1b[31m界>\x1b[0m",
        ] {
            let mut viewport = HorizontalViewport::default();
            let text = "é".repeat(30);
            let (out, col) = viewport.layout(&line(prompt, &text, 0), 12);
            assert_eq!(out, format!("\x1b[31m界>\x1b[0m{}>", "é".repeat(7)));
            assert_eq!(col, 3);
            let (out, col) = viewport.layout(&line(prompt, &text, text.len()), 12);
            assert!(out.contains("\x1b[31m"));
            assert!(out.contains("\x1b[0m"));
            assert!(visible_width(&out) < 12);
            assert!(col < 12);
        }
        // Arbitrary bytes within a hidden span are retained and not counted.
        let prompt = Prompt::new("\x01hidden\x02P>");
        assert_eq!(prompt.horizontal_line().width, prompt.width());
        assert_eq!(prompt.width(), 2);
        let mut viewport = HorizontalViewport::default();
        assert_eq!(viewport.layout(&prompt.horizontal_line(), 12).0, "hiddenP>");
    }

    #[test]
    fn horizontal_scroll_never_splits_rendered_units_at_markers() {
        for (text, options, atom) in [
            ("界", RenderOptions::default(), "界"),
            ("e\u{301}", RenderOptions::default(), "e\u{301}"),
            ("\x01", RenderOptions::default(), "^A"),
            (
                "é",
                RenderOptions {
                    output_meta: false,
                    byte_oriented: true,
                    ..RenderOptions::default()
                },
                "\\303\\251",
            ),
        ] {
            let buffer = LineBuffer::from(&text.repeat(20));
            let mut line = buffer.horizontal_line(None, options, 0);
            for columns in 1..25 {
                for point in 0..=line.width {
                    line.point = point;
                    let mut viewport = HorizontalViewport::default();
                    let (out, col) = viewport.layout(&line, columns);
                    assert!(visible_width(&out) < columns);
                    assert!(col < columns);
                    let remainder = out.replace(atom, "");
                    assert!(remainder.chars().all(|ch| " <>".contains(ch)), "{out:?}");
                }
            }
        }
    }

    #[test]
    fn horizontal_scroll_uses_prompt_tab_basis_and_keeps_prompt_prefix_lines() {
        let mut viewport = HorizontalViewport::default();
        let layout = line("title\nP>", "a\tb", 3);
        assert_eq!(viewport.layout(&layout, 20), ("title\nP>a     b".into(), 9));
    }
}
