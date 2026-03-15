use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

pub fn success(msg: &str) {
    println!("{} {}", "✓".green().bold(), msg);
}

pub fn error(msg: &str) {
    eprintln!("{} {}", "✗".red().bold(), msg);
}

pub fn progress(msg: &str) {
    println!("{} {}", "▶".cyan().bold(), msg);
}

pub fn hint(msg: &str) {
    eprintln!("  {}", format!("hint: {}", msg).dimmed());
}

pub fn divider() {
    println!("{}", "─".repeat(60).dimmed().to_string());
}

/// Truncate a string to at most `max` chars, appending "..." if cut.
pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut = max.saturating_sub(3);
        let trimmed: String = s.chars().take(cut).collect();
        format!("{}...", trimmed)
    }
}

pub struct Spinner(ProgressBar);

impl Spinner {
    pub fn new(msg: &str) -> Self {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::with_template("{spinner:.cyan} {msg}")
                .unwrap()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        pb.set_message(msg.to_string());
        pb.enable_steady_tick(Duration::from_millis(100));
        Self(pb)
    }

    pub fn set_message(&self, msg: impl Into<String>) {
        self.0.set_message(msg.into());
    }

    pub fn finish_success(&self, msg: &str) {
        self.0.finish_and_clear();
        success(msg);
    }

    pub fn finish_error(&self, msg: &str) {
        self.0.finish_and_clear();
        error(msg);
    }
}

/// Print an aligned table. `rows` is a slice of rows, each a Vec of cell strings.
pub fn print_table(headers: &[&str], rows: &[Vec<String>]) {
    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < widths.len() {
                widths[i] = widths[i].max(cell.chars().count());
            }
        }
    }

    let fmt_row = |cells: &[String]| -> String {
        cells
            .iter()
            .zip(widths.iter())
            .map(|(c, w)| format!("{:<width$}", c, width = w))
            .collect::<Vec<_>>()
            .join("  ")
    };

    let header_cells: Vec<String> = headers.iter().map(|h| h.to_string()).collect();
    println!("  {}", fmt_row(&header_cells).bold());

    let sep_cells: Vec<String> = widths.iter().map(|w| "─".repeat(*w)).collect();
    println!("  {}", fmt_row(&sep_cells).dimmed());

    for row in rows {
        println!("  {}", fmt_row(row));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_string_unchanged() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_exact_length_unchanged() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn truncate_long_string_appends_ellipsis() {
        let result = truncate("hello world this is long", 10);
        assert!(result.len() <= 10, "result: {result}");
        assert!(result.ends_with("..."), "result: {result}");
    }

    #[test]
    fn truncate_very_short_max() {
        let result = truncate("hello", 3);
        assert!(result.len() <= 3, "result: {result}");
    }

    #[test]
    fn print_table_does_not_panic_with_empty_rows() {
        // Just verifies no panic
        print_table(&["A", "B", "C"], &[]);
    }

    #[test]
    fn print_table_does_not_panic_with_rows() {
        print_table(
            &["Name", "Value"],
            &[
                vec!["foo".to_string(), "bar".to_string()],
                vec!["a longer name".to_string(), "a longer value".to_string()],
            ],
        );
    }
}
