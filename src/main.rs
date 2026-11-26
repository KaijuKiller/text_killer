//! text_killer — a terminal text editor with an IRC-inspired interface.
//!
//! Usage: `text_killer [FILE]...`

mod app;
mod browser;
mod clipboard;
mod config;
mod docformats;
mod document;
mod fileio;
mod format;
mod input;
mod menu;
mod syntax;
mod theme;
mod ui;
mod wrap;

use std::io::{self, Stdout};
use std::time::Duration;

use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::cursor::SetCursorStyle;
use ratatui::crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, Event,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::Terminal;

use crate::app::App;

type Term = Terminal<CrosstermBackend<Stdout>>;

fn setup_terminal() -> io::Result<Term> {
    terminal::enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen, EnableBracketedPaste, EnableMouseCapture)?;
    Terminal::new(CrosstermBackend::new(out))
}

fn restore_terminal() {
    // Best effort: every step is attempted even if an earlier one fails.
    let _ = terminal::disable_raw_mode();
    let _ = execute!(
        io::stdout(),
        DisableMouseCapture,
        DisableBracketedPaste,
        LeaveAlternateScreen,
        SetCursorStyle::DefaultUserShape,
        ratatui::crossterm::cursor::Show
    );
}

fn print_usage() {
    println!(
        "text_killer {} — terminal text editor with an IRC-style interface\n\n\
         USAGE:\n    text_killer [FILE]...\n    text_killer --cat FILE\n    text_killer --convert INPUT OUTPUT\n\n\
         Opens each FILE in its own window (tab). Files that do not exist are\n\
         created on first save. Press F1 inside the editor for key bindings.\n\n\
         OPTIONS:\n\
         \x20   --cat FILE              Print FILE as text (documents such as .odt, .docx,\n\
         \x20                           .epub, .xlsx are converted) and exit\n\
         \x20   --convert INPUT OUTPUT  Convert between formats by extension, e.g.\n\
         \x20                           notes.md -> notes.odt, report.docx -> report.rtf\n\
         \x20   -h, --help              Print this help\n\
         \x20   -V, --version           Print version",
        env!("CARGO_PKG_VERSION")
    );
}

/// `--cat`: prints a file as text.
fn cat(path: &str) -> Result<(), String> {
    let doc = fileio::load(&fileio::expand_tilde(path)).map_err(|e| format!("{path}: {e}"))?;
    use std::io::Write;
    let mut out = io::stdout().lock();
    let mut text = doc.lines.join("\n");
    if !text.ends_with('\n') {
        text.push('\n');
    }
    out.write_all(text.as_bytes()).map_err(|e| e.to_string())
}

/// `--convert`: loads INPUT and saves it as OUTPUT (format by extension).
fn convert(input: &str, output: &str) -> Result<(), String> {
    let doc = fileio::load(&fileio::expand_tilde(input)).map_err(|e| format!("{input}: {e}"))?;
    if doc.encoding == fileio::Encoding::Binary {
        return Err(format!("{input}: binary files cannot be converted"));
    }
    let out = fileio::expand_tilde(output);
    let mut doc = doc;
    // Text written to a plain file uses UTF-8 unless it came from one.
    if doc.format.is_some() {
        doc.encoding = fileio::Encoding::Utf8;
    }
    let bytes = fileio::save(&doc, &out).map_err(|e| format!("{output}: {e}"))?;
    eprintln!("text_killer: wrote {output} ({bytes} bytes)");
    Ok(())
}

fn run(term: &mut Term, app: &mut App) -> io::Result<()> {
    let mut overwrite_shape = None;
    let mut mouse_on = app.settings.mouse;
    loop {
        term.draw(|f| ui::draw(f, app))?;

        // Block cursor for overwrite mode, bar for insert.
        let ow = app.doc().overwrite;
        if overwrite_shape != Some(ow) {
            let shape = if ow { SetCursorStyle::SteadyBlock } else { SetCursorStyle::SteadyBar };
            execute!(io::stdout(), shape)?;
            overwrite_shape = Some(ow);
        }
        if app.mouse_changed || mouse_on != app.settings.mouse {
            app.mouse_changed = false;
            mouse_on = app.settings.mouse;
            if mouse_on {
                execute!(io::stdout(), EnableMouseCapture)?;
            } else {
                execute!(io::stdout(), DisableMouseCapture)?;
            }
        }

        if app.quit {
            return Ok(());
        }

        // Wait for input; wake periodically to expire status messages.
        if !event::poll(Duration::from_millis(500))? {
            app.tick();
            continue;
        }
        // Drain all pending events before redrawing (fast typing / pastes).
        loop {
            match event::read()? {
                // The layout is recomputed from the new size on the next draw.
                Event::Resize(_, _) => term.autoresize()?,
                ev => app.handle_event(ev),
            }
            if app.quit || !event::poll(Duration::ZERO)? {
                break;
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut files = Vec::new();
    let mut only_files = false;
    for a in args {
        match a.as_str() {
            "--" if !only_files => only_files = true,
            "-h" | "--help" if !only_files => {
                print_usage();
                return;
            }
            "--cat" | "--convert" if !only_files => {
                let rest: Vec<String> = std::env::args().skip_while(|x| x != &a).skip(1).collect();
                let result = match (a.as_str(), rest.as_slice()) {
                    ("--cat", [f, ..]) => cat(f),
                    ("--convert", [i, o, ..]) => convert(i, o),
                    _ => Err(format!("{a}: missing file argument (try --help)")),
                };
                if let Err(e) = result {
                    eprintln!("text_killer: {e}");
                    std::process::exit(1);
                }
                return;
            }
            "-V" | "--version" if !only_files => {
                println!("text_killer {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            s if s.starts_with('-') && !only_files && s.len() > 1 => {
                eprintln!("text_killer: unknown option '{s}' (try --help)");
                std::process::exit(2);
            }
            _ => files.push(a),
        }
    }

    // Restore the terminal before printing any panic message.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        default_hook(info);
    }));

    let mut app = App::new(&files);
    let mut term = match setup_terminal() {
        Ok(t) => t,
        Err(e) => {
            restore_terminal();
            eprintln!("text_killer: failed to initialise terminal: {e}");
            std::process::exit(1);
        }
    };
    let result = run(&mut term, &mut app);
    restore_terminal();
    if let Err(e) = result {
        eprintln!("text_killer: {e}");
        std::process::exit(1);
    }
}
