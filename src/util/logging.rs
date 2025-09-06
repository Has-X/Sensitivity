// Copyright (C) 2025 HasX
// Licensed under the GNU AGPL v3.0. See LICENSE file for details.
// Website: https://hasx.dev

#[derive(Copy, Clone, Debug)]
pub enum LogVerbosity {
    Normal,
    Verbose,
    Debug,
}

pub fn init_logger(verbosity: LogVerbosity) {
    let level = match verbosity {
        LogVerbosity::Normal => log::LevelFilter::Info,
        LogVerbosity::Verbose => log::LevelFilter::Debug,
        LogVerbosity::Debug => log::LevelFilter::Trace,
    };
    let _ = env_logger::Builder::from_default_env()
        .filter_level(level)
        .format_timestamp_millis()
        .try_init();
}

