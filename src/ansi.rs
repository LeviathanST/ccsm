use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
};
use vt100::Parser;

/// Wraps a `vt100::Parser` to convert terminal output to ratatui `Text`.
pub struct TerminalScreen {
    parser: Parser,
}

impl TerminalScreen {
    pub fn new(rows: u16, cols: u16) -> Self {
        Self {
            parser: Parser::new(rows, cols, 0),
        }
    }

    /// Feed bytes from the PTY into the terminal parser.
    pub fn process(&mut self, data: &[u8]) {
        self.parser.process(data);
    }

    /// Resize the virtual terminal grid.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.parser.screen_mut().set_size(rows, cols);
    }

    /// Convert the current screen state to ratatui `Text` with styling.
    ///
    /// Iterates every cell in the fixed grid (rows × cols).
    /// Adjacent cells with identical style are merged into a single Span.
    pub fn render(&self) -> Text<'_> {
        let screen = self.parser.screen();
        let (rows, cols) = screen.size();
        let mut lines: Vec<Line<'_>> = Vec::with_capacity(rows as usize + 1);

        for row in 0..rows {
            let mut spans: Vec<Span> = Vec::new();
            let mut current_style = Style::default();
            let mut current_text = String::new();

            for col in 0..cols {
                let (ch, style) = match screen.cell(row, col) {
                    Some(cell) if cell.has_contents() => {
                        (cell.contents(), cell_style(cell))
                    }
                    _ => {
                        (" ", Style::default())
                    }
                };

                if style != current_style {
                    if !current_text.is_empty() {
                        spans.push(Span::styled(
                            std::mem::take(&mut current_text),
                            current_style,
                        ));
                    }
                    current_style = style;
                }
                current_text.push_str(ch);
            }

            if !current_text.is_empty() {
                spans.push(Span::styled(current_text, current_style));
            }

            if spans.is_empty() {
                spans.push(Span::raw(" "));
            }

            lines.push(Line::from(spans));
        }

        Text::from(lines)
    }
}

/// Convert vt100 cell attributes to a ratatui `Style`.
fn cell_style(cell: &vt100::Cell) -> Style {
    let fg = ansi_color_to_ratatui(cell.fgcolor());
    let bg = ansi_color_to_ratatui(cell.bgcolor());

    let mut style = Style::default().fg(fg);

    if bg != Color::Reset {
        style = style.bg(bg);
    }

    if cell.bold() {
        style = style.add_modifier(Modifier::BOLD);
    }
    if cell.italic() {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if cell.underline() {
        style = style.add_modifier(Modifier::UNDERLINED);
    }

    // Handle reverse video
    if cell.inverse() {
        style = Style::default().fg(Color::Black).bg(Color::White);
    }

    style
}

/// Map vt100's `Color` enum (which wraps ANSI color values) to ratatui `Color`.
fn ansi_color_to_ratatui(color: vt100::Color) -> Color {
    use vt100::Color as VtColor;
    match color {
        VtColor::Default => Color::Reset,
        VtColor::Idx(i) => match i {
            0 => Color::Black,
            1 => Color::Red,
            2 => Color::Green,
            3 => Color::Yellow,
            4 => Color::Blue,
            5 => Color::Magenta,
            6 => Color::Cyan,
            7 => Color::White,
            8 => Color::Rgb(128, 128, 128),   // Bright black
            9 => Color::Rgb(255, 128, 128),   // Bright red
            10 => Color::Rgb(128, 255, 128),  // Bright green
            11 => Color::Rgb(255, 255, 128),  // Bright yellow
            12 => Color::Rgb(128, 128, 255),  // Bright blue
            13 => Color::Rgb(255, 128, 255),  // Bright magenta
            14 => Color::Rgb(128, 255, 255),  // Bright cyan
            15 => Color::Rgb(255, 255, 255),  // Bright white
            _ => Color::Reset,
        },
        VtColor::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}
