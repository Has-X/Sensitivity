// Copyright (C) 2026 HasX
// Licensed under the GNU AGPL v3.0. See LICENSE file for details.

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use clap::{Parser, ValueEnum};

#[derive(Clone, Copy, ValueEnum)]
enum Theme {
    System,
    Light,
    Dark,
}

#[derive(Parser)]
#[command(about = "Sensitivity graphical Recovery ROM installer")]
struct Args {
    /// Enable the safe offline GUI demo flow
    #[arg(long)]
    demo: bool,
    /// Override the GUI colour theme at startup
    #[arg(long, value_enum, default_value_t = Theme::System)]
    theme: Theme,
}

fn main() -> eframe::Result {
    let args = Args::parse();
    let theme = match args.theme {
        Theme::System => sensitivity::gui::ThemeOverride::System,
        Theme::Light => sensitivity::gui::ThemeOverride::Light,
        Theme::Dark => sensitivity::gui::ThemeOverride::Dark,
    };
    sensitivity::gui::run(args.demo, theme)
}
