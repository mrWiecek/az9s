use super::theme::theme;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, StatefulWidget, Widget},
};

/// A selectable list widget with scrolling support
#[derive(Debug, Clone)]
pub struct SelectableList<'a> {
    items: Vec<ListItem<'a>>,
    block: Option<Block<'a>>,
    highlight_style: Style,
    normal_style: Style,
}

/// A single item in a selectable list
#[derive(Debug, Clone)]
pub struct ListItem<'a> {
    content: Line<'a>,
}

impl<'a> ListItem<'a> {
    pub fn new<T>(content: T) -> Self
    where
        T: Into<Line<'a>>,
    {
        Self {
            content: content.into(),
        }
    }
}

impl<'a> SelectableList<'a> {
    pub fn new(items: Vec<ListItem<'a>>) -> Self {
        Self {
            items,
            block: None,
            highlight_style: theme().selection(),
            normal_style: Style::default(),
        }
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    pub fn highlight_style(mut self, style: Style) -> Self {
        self.highlight_style = style;
        self
    }

    pub fn normal_style(mut self, style: Style) -> Self {
        self.normal_style = style;
        self
    }
}

/// State for the selectable list
#[derive(Debug, Clone, Default)]
pub struct SelectableListState {
    pub selected: usize,
    pub offset: usize,
}

impl SelectableListState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn select(&mut self, index: usize) {
        self.selected = index;
    }
}

impl<'a> StatefulWidget for SelectableList<'a> {
    type State = SelectableListState;

    fn render(mut self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let list_area = match self.block.take() {
            Some(b) => {
                let inner_area = b.inner(area);
                b.render(area, buf);
                inner_area
            }
            None => area,
        };

        if list_area.height < 1 {
            return;
        }

        let list_height = list_area.height as usize;

        // Update offset to ensure selected item is visible
        if state.selected < state.offset {
            state.offset = state.selected;
        } else if state.selected >= state.offset + list_height {
            state.offset = state.selected.saturating_sub(list_height - 1);
        }

        // Render visible items
        for (i, item) in self
            .items
            .iter()
            .enumerate()
            .skip(state.offset)
            .take(list_height)
        {
            let y = list_area.y + (i - state.offset) as u16;
            let item_area = Rect {
                x: list_area.x,
                y,
                width: list_area.width,
                height: 1,
            };

            let style = if i == state.selected {
                self.highlight_style
            } else {
                self.normal_style
            };

            // Render the item with appropriate style
            let mut x = item_area.x;
            for span in &item.content.spans {
                let span_style = style.patch(span.style);
                let content = &span.content;

                for ch in content.chars() {
                    if x >= item_area.x + item_area.width {
                        break;
                    }
                    buf[(x, y)].set_char(ch).set_style(span_style);
                    x += 1;
                }
            }

            // Fill the rest of the line with the style
            while x < item_area.x + item_area.width {
                buf[(x, y)].set_style(style);
                x += 1;
            }
        }
    }
}

/// Shorten `s` to at most `max` characters, ending in "..." when cut.
///
/// Counts characters, not bytes, so names with multi-byte UTF-8 characters
/// never cause a panic.
/// A fixed-width column: `text` cut with "…" so at least one space is
/// left before the next column, then padded to `width` characters.
pub fn cell(text: &str, width: usize) -> String {
    if width < 2 {
        return " ".repeat(width);
    }
    let room = width - 1;
    let shown = if text.chars().count() > room {
        let mut cut: String = text.chars().take(room - 1).collect();
        cut.push('…');
        cut
    } else {
        text.to_string()
    };
    format!("{shown:<width$}")
}

/// Width of a list's flexible (name) column: what is left of `area_width`
/// after the borders and the `fixed` columns, kept between 20 and `max`.
pub fn name_width(area_width: u16, fixed: usize, max: usize) -> usize {
    (area_width as usize)
        .saturating_sub(2 + 2 + fixed) // borders, highlight marker
        .clamp(20, max)
}

pub fn truncate_with_ellipsis(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let keep = max.saturating_sub(3);
    let mut out: String = s.chars().take(keep).collect();
    out.push_str("...");
    out
}

/// Helper function to create a list item from a string
pub fn list_item<'a, S: Into<String>>(s: S) -> ListItem<'a> {
    ListItem::new(Line::from(s.into()))
}

/// Helper function to create a list item with styled spans
pub fn list_item_spans<'a>(spans: Vec<Span<'a>>) -> ListItem<'a> {
    ListItem::new(Line::from(spans))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_always_leave_a_gap() {
        assert_eq!(cell("short", 8), "short   ");
        assert_eq!(cell("exactly8", 8), "exactl… ");
        assert_eq!(cell("a-very-long-resource-name", 10), "a-very-l… ");
        assert_eq!(cell("ąęółżźćń-długa", 6), "ąęół… ", "characters, not bytes");
        assert_eq!(cell("x", 0), "");
        assert_eq!(cell("xy", 1), " ");
    }

    #[test]
    fn name_column_grows_with_the_terminal() {
        assert_eq!(name_width(80, 32, 80), 44);
        assert_eq!(name_width(40, 32, 80), 20, "never below 20");
        assert_eq!(name_width(300, 32, 80), 80, "never above max");
    }

    #[test]
    fn truncate_keeps_short_strings() {
        assert_eq!(truncate_with_ellipsis("vm-01", 35), "vm-01");
    }

    #[test]
    fn truncate_cuts_long_strings_to_max() {
        let out = truncate_with_ellipsis(&"a".repeat(50), 35);
        assert_eq!(out.chars().count(), 35);
        assert!(out.ends_with("..."));
    }

    #[test]
    fn truncate_is_utf8_safe() {
        // Byte slicing at 32 would land inside a multi-byte character and panic.
        let name = "ż".repeat(40);
        let out = truncate_with_ellipsis(&name, 35);
        assert_eq!(out.chars().count(), 35);
    }
}
