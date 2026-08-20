// Copyright (C) 2026 HasX
// Licensed under the GNU AGPL v3.0. See LICENSE file for details.
// Website: https://hasx.dev

//! Core implementation for the Sensitivity Xiaomi Recovery tool.
//!
//! The library contains USB transport, ADB framing, Xiaomi recovery commands,
//! validation, download, and sideload behavior. User interaction and argument
//! parsing live in the `sensitivity` binary.

pub mod adb;
pub mod download;
pub mod mi;
pub mod sideload;
pub mod usb;
pub mod util;
pub mod validate;
