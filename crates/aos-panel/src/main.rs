//! `aos-panel`, the command panel as a program.
//!
//! The window is the whole interface, and everything it knows lives in `aos_panel::App`. This
//! file only parses the arguments, opens the window and pumps the loop.
//!
//! `--frames` and `--seconds` draw a bounded number of real frames and exit. That is what
//! makes the panel checkable without a person: unit tests exercise the state and never the
//! layout, and a panel breaks in the layout, in a row that will not fit or a scroll area
//! nested inside itself. None of that shows up until something actually paints it.

use std::path::PathBuf;
use std::time::Instant;

use aos_panel::{App, report, theme, ui};
use clap::Parser;
use eframe::egui::ViewportCommand;

#[derive(Parser)]
#[command(name = "aos-panel", version, about = "The AOS command panel")]
struct Args {
    /// Where the event log, allowlist, agent output and socket live. The same default as the
    /// cli, so pointing them at the same place takes no arguments at all.
    #[arg(long, default_value = "run")]
    run_dir: PathBuf,

    /// Draw this many frames and exit, printing what was on screen.
    #[arg(long)]
    frames: Option<u32>,

    /// Run for this many seconds and exit, printing what was on screen every second.
    ///
    /// Separate from `--frames` because the two prove different things. A frame count says the
    /// layout painted. Elapsed time says records appended while it was running actually
    /// reached the screen.
    #[arg(long)]
    seconds: Option<u64>,
}

fn main() -> eframe::Result<()> {
    let args = Args::parse();
    let mut app = App::new(args.run_dir.clone());
    println!(
        "watching {} read only. Commands go to {}",
        app.ledger_path().display(),
        aos_cli::client::socket_path(&args.run_dir).display()
    );

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("AOS Command Panel")
            .with_inner_size([1500.0, 900.0]),
        ..Default::default()
    };

    let began = Instant::now();
    let mut drawn = 0u32;
    let mut said = 0u64;
    // The close command does not take effect until the next frame, so without this the final
    // line gets printed twice and the second one is a frame further on than the run asked for.
    let mut reported = false;

    eframe::run_simple_native("aos-panel", options, move |ctx, _frame| {
        // Every frame rather than once. egui reapplies the desktop's light or dark preference,
        // and this desktop is in light mode, which paints every label that does not name its
        // own colour black on near black.
        theme::install(ctx);

        app.tick();
        ui::draw(&mut app, ctx);
        drawn += 1;

        if args.seconds.is_some() {
            let elapsed = began.elapsed().as_secs();
            if elapsed > said {
                said = elapsed;
                println!("{elapsed:>3}s  {}", report::summary(&app, drawn));
            }
        }

        let done = args.seconds.is_some_and(|s| began.elapsed().as_secs() >= s)
            || args.frames.is_some_and(|f| drawn >= f);
        if done && !reported {
            reported = true;
            println!("{}", report::summary(&app, drawn));
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }

        // The ledger is polled, so the panel redraws on a clock rather than only on input.
        // Modest, so an idle panel is not a busy loop.
        ctx.request_repaint_after(std::time::Duration::from_millis(150));
    })
}
