use console::Style;
use std::fmt::Display;
use std::io::{self, BufRead, Write};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptDefault {
    Yes,
    No,
}

#[derive(Clone)]
pub struct UiStyles {
    pub heading_prefix: Style,
    pub note_label: Style,
    pub info_label: Style,
    pub warn_label: Style,
    pub error_label: Style,
    pub bullet: Style,
    pub step_pending: Style,
    pub step_ok: Style,
    pub step_fail: Style,
}

impl Default for UiStyles {
    fn default() -> Self {
        Self {
            heading_prefix: Style::new().cyan().bold(),
            note_label: Style::new().yellow().bold(),
            info_label: Style::new().cyan().bold(),
            warn_label: Style::new().yellow().bold(),
            error_label: Style::new().red().bold(),
            bullet: Style::new(),
            step_pending: Style::new().dim(),
            step_ok: Style::new().green(),
            step_fail: Style::new().red(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiSymbols {
    pub heading_prefix: &'static str,
    pub note_label: &'static str,
    pub info_label: &'static str,
    pub warn_label: &'static str,
    pub error_label: &'static str,
    pub bullet: &'static str,
    pub step_pending: &'static str,
    pub step_ok: &'static str,
    pub step_fail: &'static str,
}

impl Default for UiSymbols {
    fn default() -> Self {
        Self {
            heading_prefix: "==>",
            note_label: "Note:",
            info_label: "Info:",
            warn_label: "Warning:",
            error_label: "error:",
            bullet: "•",
            step_pending: "○",
            step_ok: "✓",
            step_fail: "✗",
        }
    }
}

#[derive(Clone, Default)]
pub struct UiTheme {
    pub styles: UiStyles,
    pub symbols: UiSymbols,
}

pub struct Ui<O: Write, E: Write> {
    out: O,
    err: E,
    pub theme: UiTheme,
}

impl Ui<io::Stdout, io::Stderr> {
    pub fn new() -> Self {
        Self::with_theme(UiTheme::default())
    }

    pub fn with_theme(theme: UiTheme) -> Self {
        Self {
            out: io::stdout(),
            err: io::stderr(),
            theme,
        }
    }
}

impl<O: Write, E: Write> Ui<O, E> {
    pub fn with_writers(out: O, err: E) -> Self {
        Self {
            out,
            err,
            theme: UiTheme::default(),
        }
    }

    pub fn with_theme_and_writers(theme: UiTheme, out: O, err: E) -> Self {
        Self { out, err, theme }
    }

    pub fn heading(&mut self, message: impl Display) -> io::Result<()> {
        let label = self
            .theme
            .styles
            .heading_prefix
            .apply_to(self.theme.symbols.heading_prefix)
            .to_string();
        writeln!(self.out, "{label} {message}")
    }

    pub fn note(&mut self, message: impl Display) -> io::Result<()> {
        let label = self
            .theme
            .styles
            .note_label
            .apply_to(self.theme.symbols.note_label)
            .to_string();
        writeln!(self.out, "{label} {message}")
    }

    pub fn info(&mut self, message: impl Display) -> io::Result<()> {
        let label = self
            .theme
            .styles
            .info_label
            .apply_to(self.theme.symbols.info_label)
            .to_string();
        writeln!(self.out, "{label} {message}")
    }

    pub fn warn(&mut self, message: impl Display) -> io::Result<()> {
        let label = self
            .theme
            .styles
            .warn_label
            .apply_to(self.theme.symbols.warn_label)
            .to_string();
        writeln!(self.err, "{label} {message}")
    }

    pub fn error(&mut self, message: impl Display) -> io::Result<()> {
        let label = self
            .theme
            .styles
            .error_label
            .apply_to(self.theme.symbols.error_label)
            .to_string();
        writeln!(self.err, "{label} {message}")
    }

    pub fn bullet(&mut self, message: impl Display) -> io::Result<()> {
        let symbol = self
            .theme
            .styles
            .bullet
            .apply_to(self.theme.symbols.bullet)
            .to_string();
        writeln!(self.out, "    {symbol} {message}")
    }

    pub fn step_start(&mut self, message: impl Display) -> io::Result<()> {
        let pending = self
            .theme
            .styles
            .step_pending
            .apply_to(self.theme.symbols.step_pending)
            .to_string();
        write!(self.out, "    {pending} {message}...")
    }

    pub fn step_ok(&mut self) -> io::Result<()> {
        writeln!(
            self.out,
            " {}",
            self.theme
                .styles
                .step_ok
                .apply_to(self.theme.symbols.step_ok)
        )
    }

    pub fn step_fail(&mut self) -> io::Result<()> {
        writeln!(
            self.out,
            " {}",
            self.theme
                .styles
                .step_fail
                .apply_to(self.theme.symbols.step_fail)
        )
    }

    pub fn blank_line(&mut self) -> io::Result<()> {
        writeln!(self.out)
    }

    pub fn prompt_yes_no(&mut self, prompt: &str, default: PromptDefault) -> io::Result<bool> {
        let mut stdin = io::stdin().lock();
        self.prompt_yes_no_with_reader(prompt, default, &mut stdin)
    }

    pub fn prompt_yes_no_with_reader<R: BufRead>(
        &mut self,
        prompt: &str,
        default: PromptDefault,
        reader: &mut R,
    ) -> io::Result<bool> {
        write!(self.out, "{prompt} ")?;
        self.out.flush()?;

        let mut input = String::new();
        reader.read_line(&mut input)?;

        Ok(parse_yes_no_input(&input, default))
    }
}

impl Default for Ui<io::Stdout, io::Stderr> {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_yes_no_input(input: &str, default: PromptDefault) -> bool {
    let normalized = input.trim().to_ascii_lowercase();

    if normalized.is_empty() {
        return matches!(default, PromptDefault::Yes);
    }

    matches!(normalized.as_str(), "y" | "yes")
}

#[cfg(test)]
mod tests {
    use super::{PromptDefault, Ui, UiTheme, parse_yes_no_input};
    use std::io::Cursor;

    #[test]
    fn prompt_default_yes_accepts_empty_input() {
        assert!(parse_yes_no_input("", PromptDefault::Yes));
        assert!(parse_yes_no_input("   ", PromptDefault::Yes));
    }

    #[test]
    fn prompt_default_no_rejects_empty_input() {
        assert!(!parse_yes_no_input("", PromptDefault::No));
        assert!(!parse_yes_no_input("   ", PromptDefault::No));
    }

    #[test]
    fn prompt_accepts_yes_tokens_case_insensitively() {
        assert!(parse_yes_no_input("y", PromptDefault::No));
        assert!(parse_yes_no_input("Y", PromptDefault::No));
        assert!(parse_yes_no_input("yes", PromptDefault::No));
        assert!(parse_yes_no_input("YeS", PromptDefault::No));
    }

    #[test]
    fn prompt_rejects_non_yes_tokens() {
        assert!(!parse_yes_no_input("n", PromptDefault::Yes));
        assert!(!parse_yes_no_input("no", PromptDefault::Yes));
        assert!(!parse_yes_no_input("random", PromptDefault::Yes));
    }

    #[test]
    fn prompt_with_reader_uses_default_yes_on_empty_input() {
        let mut ui = Ui::with_writers(Vec::<u8>::new(), Vec::<u8>::new());
        let mut input = Cursor::new("\n");

        let accepted = ui
            .prompt_yes_no_with_reader("Continue? [Y/n]", PromptDefault::Yes, &mut input)
            .unwrap();

        assert!(accepted);
    }

    #[test]
    fn heading_respects_theme_symbols() {
        let mut theme = UiTheme::default();
        theme.symbols.heading_prefix = "->";

        let mut ui = Ui::with_theme_and_writers(theme, Vec::<u8>::new(), Vec::<u8>::new());
        ui.heading("hello").unwrap();

        let out = String::from_utf8(ui.out).unwrap();
        assert!(out.contains("-> hello"));
    }
}
