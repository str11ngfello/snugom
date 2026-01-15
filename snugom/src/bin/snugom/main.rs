mod commands;
mod context;
mod differ;
mod examples;
mod executor;
mod generator;
mod output;
mod scanner;
mod theme;
mod utils;

use anyhow::Result;
use clap::{
    builder::{
        styling::{AnsiColor, Color as ClapColor, RgbColor, Style},
        Styles,
    },
    error::ErrorKind,
    ColorChoice, Command, CommandFactory, FromArgMatches, Parser, Subcommand,
};

use colored::{control::ShouldColorize, Color as ThemeColor, Colorize};
use std::fmt::Write;
use std::io::{self, Write as IoWrite};

use commands::{
    init::{handle_init, InitArgs},
    migrate::{handle_migrate_commands, MigrateCommands},
    schema::{handle_schema_commands, SchemaCommands},
};
use examples::{command_examples, ExampleGroup};
use output::{GlobalOptions, OutputFormat, OutputManager};
use theme::{ICONS, THEME};

const ENVIRONMENT_VARIABLES: &[(&str, &str)] = &[
    ("REDIS_URL", "Redis connection URL for migrations"),
];

#[derive(Parser)]
#[command(name = "snugom")]
#[command(author = "Snug API Team")]
#[command(version = "0.1.0")]
#[command(
    about = "Schema versioning and migration tool for SnugOM",
    long_about = r#"Schema versioning and migration CLI for SnugOM that provides:

• Automatic schema change detection (no manual version bumping)
• Maximum code generation for migrations
• Full SnugOM access during migration execution
• Clean separation: migrations run BEFORE app, runtime just validates

Commands:
  init      Initialize snugom in a project
  migrate   Generate and deploy migrations
  schema    View schema status and differences
"#
)]
#[command(subcommand_required = true, arg_required_else_help = true)]
struct Cli {
    /// Output format
    #[arg(long, value_enum, default_value = "table")]
    output: OutputFormat,

    /// Suppress output (only errors will be shown)
    #[arg(short = 'q', long)]
    quiet: bool,

    /// Enable verbose output
    #[arg(short = 'v', long)]
    verbose: bool,

    /// Disable colored output
    #[arg(long)]
    no_color: bool,

    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    fn parse_with_styles() -> Self {
        let command = build_cli_command();
        match command.styles(help_styles()).try_get_matches() {
            Ok(matches) => Cli::from_arg_matches(&matches).expect("Failed to parse CLI arguments"),
            Err(err) => match err.kind() {
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
                    let _ = print_blank_line_stdout();
                    if let Err(print_err) = err.print()
                        && print_err.kind() != io::ErrorKind::BrokenPipe
                    {
                        eprintln!("Failed to display help: {print_err}");
                    }
                    let _ = print_blank_line_stdout();
                    std::process::exit(0);
                }
                ErrorKind::MissingSubcommand => {
                    handle_missing_subcommand(err);
                }
                _ => {
                    let exit_code = err.exit_code();
                    let _ = print_blank_line_stderr();
                    if let Err(print_err) = err.print()
                        && print_err.kind() != io::ErrorKind::BrokenPipe
                    {
                        eprintln!("Failed to display error: {print_err}");
                    }
                    let _ = print_blank_line_stderr();
                    std::process::exit(exit_code);
                }
            },
        }
    }
}

fn handle_missing_subcommand(error: clap::error::Error) -> ! {
    let mut command = build_cli_command();
    let command_name = command
        .get_display_name()
        .unwrap_or_else(|| command.get_name())
        .to_string();

    let _ = print_blank_line_stderr();
    eprintln!("error: '{command_name}' requires a subcommand but one was not provided");
    let _ = print_blank_line_stderr();

    command = command.styles(help_styles());

    let mut stderr = io::stderr();
    if command.write_long_help(&mut stderr).is_ok() {
        let _ = IoWrite::write_all(&mut stderr, b"\n");
        let _ = IoWrite::flush(&mut stderr);
    }

    let _ = print_blank_line_stderr();
    std::process::exit(error.exit_code());
}

fn build_cli_command() -> Command {
    let use_color = detect_color_support();
    let appendix = render_top_level_appendix(use_color);
    let mut command = Cli::command().after_long_help(appendix);
    command = command.color(if use_color {
        ColorChoice::Auto
    } else {
        ColorChoice::Never
    });
    attach_command_examples(&mut command, use_color);
    command
}

fn attach_command_examples(command: &mut Command, use_color: bool) {
    for example in command_examples() {
        if let Some(subcommand) = command.find_subcommand_mut(example.name) {
            let help_text = render_examples(example.groups, use_color);
            let mut updated = subcommand.clone();
            updated = updated.after_long_help(help_text);
            *subcommand = updated;
        }
    }
}

fn render_examples(groups: &[ExampleGroup], use_color: bool) -> String {
    let theme = &THEME;
    let mut buffer = String::new();

    let heading = stylize("Examples:", theme.highlight, true, use_color);
    let _ = writeln!(buffer, "{heading}");

    for (index, group) in groups.iter().enumerate() {
        let title = stylize(group.title, theme.primary, true, use_color);
        let _ = writeln!(buffer, "  {title}");

        for command in group.commands {
            let arrow = stylize(ICONS.arrow, theme.secondary, false, use_color);
            let command_text = stylize(command, theme.secondary, false, use_color);
            let _ = writeln!(buffer, "    {arrow} {command_text}");
        }

        if index + 1 < groups.len() {
            buffer.push('\n');
        }
    }

    if !buffer.ends_with('\n') {
        buffer.push('\n');
    }

    buffer
}

fn render_top_level_appendix(use_color: bool) -> String {
    let theme = &THEME;
    let mut buffer = String::new();

    let env_heading = stylize("Environment Variables:", theme.highlight, true, use_color);
    let _ = writeln!(buffer, "{env_heading}");
    for (key, description) in ENVIRONMENT_VARIABLES {
        let key_text = stylize(key, theme.key, true, use_color);
        let value_text = stylize(description, theme.value, false, use_color);
        let _ = writeln!(buffer, "  {key_text}  {value_text}");
    }

    buffer.push('\n');

    let tip_heading = stylize("Tip:", theme.highlight, true, use_color);
    let tip_text = stylize(
        "Use 'snugom <command> --help' to view examples for each command.",
        theme.secondary,
        false,
        use_color,
    );
    let _ = writeln!(buffer, "{tip_heading} {tip_text}");

    if !buffer.ends_with('\n') {
        buffer.push('\n');
    }

    buffer
}

fn print_blank_line_stdout() -> io::Result<()> {
    let mut stdout = io::stdout();
    IoWrite::write_all(&mut stdout, b"\n")?;
    IoWrite::flush(&mut stdout)
}

fn print_blank_line_stderr() -> io::Result<()> {
    let mut stderr = io::stderr();
    IoWrite::write_all(&mut stderr, b"\n")?;
    IoWrite::flush(&mut stderr)
}

fn stylize(text: &str, color: ThemeColor, bold: bool, use_color: bool) -> String {
    if use_color {
        let styled = text.color(color);
        if bold {
            styled.bold().to_string()
        } else {
            styled.to_string()
        }
    } else {
        text.to_string()
    }
}

fn detect_color_support() -> bool {
    ShouldColorize::from_env().should_colorize()
}

fn help_styles() -> Styles {
    let theme = &THEME;
    Styles::styled()
        .usage(style_from_color(theme.primary).bold())
        .header(style_from_color(theme.highlight).bold())
        .literal(style_from_color(theme.secondary))
        .placeholder(style_from_color(theme.muted))
        .valid(style_from_color(theme.success))
        .invalid(style_from_color(theme.warning))
        .error(style_from_color(theme.error).bold())
}

fn style_from_color(color: ThemeColor) -> Style {
    Style::new().fg_color(Some(color_to_clap_color(color)))
}

fn color_to_clap_color(color: ThemeColor) -> ClapColor {
    match color {
        ThemeColor::Black => ClapColor::Ansi(AnsiColor::Black),
        ThemeColor::Red => ClapColor::Ansi(AnsiColor::Red),
        ThemeColor::Green => ClapColor::Ansi(AnsiColor::Green),
        ThemeColor::Yellow => ClapColor::Ansi(AnsiColor::Yellow),
        ThemeColor::Blue => ClapColor::Ansi(AnsiColor::Blue),
        ThemeColor::Magenta => ClapColor::Ansi(AnsiColor::Magenta),
        ThemeColor::Cyan => ClapColor::Ansi(AnsiColor::Cyan),
        ThemeColor::White => ClapColor::Ansi(AnsiColor::White),
        ThemeColor::BrightBlack => ClapColor::Ansi(AnsiColor::BrightBlack),
        ThemeColor::BrightRed => ClapColor::Ansi(AnsiColor::BrightRed),
        ThemeColor::BrightGreen => ClapColor::Ansi(AnsiColor::BrightGreen),
        ThemeColor::BrightYellow => ClapColor::Ansi(AnsiColor::BrightYellow),
        ThemeColor::BrightBlue => ClapColor::Ansi(AnsiColor::BrightBlue),
        ThemeColor::BrightMagenta => ClapColor::Ansi(AnsiColor::BrightMagenta),
        ThemeColor::BrightCyan => ClapColor::Ansi(AnsiColor::BrightCyan),
        ThemeColor::BrightWhite => ClapColor::Ansi(AnsiColor::BrightWhite),
        ThemeColor::TrueColor { r, g, b } => ClapColor::Rgb(RgbColor(r, g, b)),
    }
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize snugom in the current project
    Init(InitArgs),

    /// Generate and deploy schema migrations
    #[command(subcommand)]
    Migrate(MigrateCommands),

    /// View schema status, differences, and validate data
    #[command(subcommand)]
    Schema(SchemaCommands),
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let cli = Cli::parse_with_styles();

    let _ = print_blank_line_stdout();

    match execute(cli).await {
        Ok(()) => {
            let _ = print_blank_line_stdout();
        }
        Err(err) => {
            eprintln!("Error: {err}");
            let _ = print_blank_line_stdout();
            std::process::exit(1);
        }
    }
}

async fn execute(cli: Cli) -> Result<()> {
    let global_options = GlobalOptions {
        output_format: cli.output,
        quiet: cli.quiet,
        verbose: cli.verbose,
        no_color: cli.no_color,
    };

    let output = OutputManager::new(global_options);

    match cli.command {
        Commands::Init(args) => {
            handle_init(args, &output).await?;
        }
        Commands::Migrate(migrate_cmd) => {
            handle_migrate_commands(migrate_cmd, &output).await?;
        }
        Commands::Schema(schema_cmd) => {
            handle_schema_commands(schema_cmd, &output).await?;
        }
    }

    Ok(())
}
