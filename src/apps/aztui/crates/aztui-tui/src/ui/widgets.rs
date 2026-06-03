use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
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
            highlight_style: Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
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
                    buf.get_mut(x, y)
                        .set_char(ch)
                        .set_style(span_style);
                    x += 1;
                }
            }

            // Fill the rest of the line with the style
            while x < item_area.x + item_area.width {
                buf.get_mut(x, y).set_style(style);
                x += 1;
            }
        }
    }
}

/// Helper function to create a list item from a string
pub fn list_item<'a, S: Into<String>>(s: S) -> ListItem<'a> {
    ListItem::new(Line::from(s.into()))
}

/// Helper function to create a list item with styled spans
pub fn list_item_spans<'a>(spans: Vec<Span<'a>>) -> ListItem<'a> {
    ListItem::new(Line::from(spans))
}
