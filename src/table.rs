use std::fmt::Write;

/// A simple column-based table printer for CLI output.
///
/// Handles ANSI-colored text correctly by measuring display width
/// without ANSI escape sequences. Supports left/right alignment,
/// configurable column widths, indentation, and separator.
///
/// # Example
/// ```ignore
/// Table::new()
///     .col(12)
///     .col(30)
///     .col(0)
///     .header(&["Status", "Name", "Goal"])
///     .add_row(&[status_label("in_progress"), "my-session", "fix bug"])
///     .print();
/// ```
pub(crate) struct Table {
    cols: Vec<Column>,
    rows: Vec<Vec<String>>,
    has_header: bool,
    sep: String,
    indent: String,
}

struct Column {
    width: usize,
    align: Alignment,
}

#[derive(Clone, Copy)]
enum Alignment {
    Left,
    Right,
}

impl Table {
    pub fn new() -> Self {
        Self {
            cols: Vec::new(),
            rows: Vec::new(),
            has_header: false,
            sep: "  ".to_string(),
            indent: String::new(),
        }
    }

    /// Add a column with the given width (0 = auto/remaining).
    pub fn col(&mut self, width: usize) -> &mut Self {
        self.cols.push(Column {
            width,
            align: Alignment::Left,
        });
        self
    }

    /// Add a right-aligned column.
    pub fn col_right(&mut self, width: usize) -> &mut Self {
        self.cols.push(Column {
            width,
            align: Alignment::Right,
        });
        self
    }

    /// Set column separator (default: two spaces).
    pub fn separator(&mut self, sep: &str) -> &mut Self {
        self.sep = sep.to_string();
        self
    }

    /// Set row indentation prefix.
    pub fn indent(&mut self, indent: &str) -> &mut Self {
        self.indent = indent.to_string();
        self
    }

    /// Add a header row (printed underlined).
    pub fn header(&mut self, headers: &[&str]) -> &mut Self {
        self.has_header = true;
        self.add_row_strs(headers);
        self
    }

    /// Add a data row from string references.
    pub fn add_row(&mut self, cells: &[&str]) -> &mut Self {
        self.add_row_strs(cells);
        self
    }

    fn add_row_strs(&mut self, cells: &[&str]) -> &mut Self {
        let row: Vec<String> = cells.iter().map(|c| c.to_string()).collect();
        self.rows.push(row);
        self
    }

    /// Print the table to stdout.
    pub fn print(&self) {
        for (i, row) in self.rows.iter().enumerate() {
            print!("{}", self.indent);
            let mut line = String::new();
            for (j, cell) in row.iter().enumerate() {
                if j > 0 {
                    line.push_str(&self.sep);
                }
                let col = &self.cols[j.min(self.cols.len() - 1)];
                if col.width > 0 {
                    let display_w = display_width(cell);
                    if display_w >= col.width {
                        write!(line, "{}", cell).unwrap();
                    } else {
                        let pad = col.width - display_w;
                        match col.align {
                            Alignment::Left => write!(line, "{}{}", cell, " ".repeat(pad)).unwrap(),
                            Alignment::Right => {
                                write!(line, "{}{}", " ".repeat(pad), cell).unwrap()
                            }
                        }
                    }
                } else {
                    write!(line, "{}", cell).unwrap();
                }
            }
            if self.has_header && i == 0 {
                // Underline the header
                let plain = strip_ansi(&line);
                println!("{}", line);
                println!("{}{}", self.indent, "-".repeat(plain.len()));
            } else {
                println!("{}", line);
            }
        }
    }

    /// Print the table to stderr.
    pub fn eprint(&self) {
        for (i, row) in self.rows.iter().enumerate() {
            eprint!("{}", self.indent);
            let mut line = String::new();
            for (j, cell) in row.iter().enumerate() {
                if j > 0 {
                    line.push_str(&self.sep);
                }
                let col = &self.cols[j.min(self.cols.len() - 1)];
                if col.width > 0 {
                    let display_w = display_width(cell);
                    if display_w >= col.width {
                        write!(line, "{}", cell).unwrap();
                    } else {
                        let pad = col.width - display_w;
                        match col.align {
                            Alignment::Left => write!(line, "{}{}", cell, " ".repeat(pad)).unwrap(),
                            Alignment::Right => {
                                write!(line, "{}{}", " ".repeat(pad), cell).unwrap()
                            }
                        }
                    }
                } else {
                    write!(line, "{}", cell).unwrap();
                }
            }
            if self.has_header && i == 0 {
                let plain = strip_ansi(&line);
                eprintln!("{}", line);
                eprintln!("{}{}", self.indent, "-".repeat(plain.len()));
            } else {
                eprintln!("{}", line);
            }
        }
    }
}

/// Display width of a string, ignoring ANSI escape sequences.
fn display_width(s: &str) -> usize {
    strip_ansi(s).len()
}

/// Strip ANSI escape sequences from a string.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_escape = false;
    for ch in s.chars() {
        if in_escape {
            if ch == 'm' {
                in_escape = false;
            }
            continue;
        }
        if ch == '\x1b' {
            in_escape = true;
            continue;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_ansi_removes_simple_escape() {
        assert_eq!(strip_ansi("\x1b[31mred\x1b[0m"), "red");
    }

    #[test]
    fn strip_ansi_passes_plain_text() {
        assert_eq!(strip_ansi("hello world"), "hello world");
    }

    #[test]
    fn strip_ansi_handles_empty() {
        assert_eq!(strip_ansi(""), "");
    }

    #[test]
    fn strip_ansi_handles_multiple_escapes() {
        assert_eq!(strip_ansi("\x1b[1m\x1b[32mbold green\x1b[0m"), "bold green");
    }

    #[test]
    fn display_width_strips_ansi() {
        let colored = "\x1b[34mhello\x1b[0m";
        assert_eq!(display_width(colored), 5);
    }

    #[test]
    fn display_width_plain_text() {
        assert_eq!(display_width("hello"), 5);
    }

    #[test]
    fn table_one_column() {
        let mut t = Table::new();
        t.col(10).add_row(&["hello"]);
        // Just verify it doesn't panic — output goes to stdout
        t.print();
    }

    #[test]
    fn table_two_columns_auto_width() {
        let mut t = Table::new();
        t.col(5).col(0).add_row(&["left", "right content"]);
        t.print();
    }

    #[test]
    fn table_ansi_in_cell() {
        let mut t = Table::new();
        t.col(10).col(0).add_row(&["\x1b[31mred\x1b[0m", "text"]);
        // Display width of "red" = 3, padded to 10 → "red       "
        t.print();
    }

    #[test]
    fn table_indent_and_separator() {
        let mut t = Table::new();
        t.indent("> ")
            .separator(" | ")
            .col(5)
            .col(0)
            .add_row(&["a", "b"]);
        t.print();
    }

    #[test]
    fn table_multiple_rows_same_widths() {
        let mut t = Table::new();
        t.col(8).col(8).col(0);
        t.add_row(&["col1", "col2", "col3"]);
        t.add_row(&["a", "b", "c"]);
        t.print();
    }

    #[test]
    fn eprint_does_not_panic() {
        let mut t = Table::new();
        t.col(5).col(0).add_row(&["hello", "world"]);
        t.eprint();
    }
}
