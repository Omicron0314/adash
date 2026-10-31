//! adash desktop app library.
//!
//! Module layout follows docs/ARCHITECTURE.md: `app` (shell + state + routing),
//! `views_*` (one module per page), `widgets` (thin Material compositions the
//! kit lacks), `agent` (pi process + conversation), `i18n` (all copy).

extern crate gpui_kit as gpui;

pub mod agent;
pub mod app;
pub mod i18n;
pub mod palette;
pub mod ssh;
mod terminal_input;
pub mod terminal_view;
pub mod views_files;
pub mod views_flows;
pub mod views_home;
pub mod views_resources;
pub mod views_sessions;
pub mod views_settings;
pub mod widgets;
