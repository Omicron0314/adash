//! adash desktop app library.
//!
//! Module layout is fixed by docs/ARCHITECTURE.md:
//! `app` (shell + routing), `views` (one module per page), `widgets`
//! (thin Material compositions this app needs but the kit lacks), `agent`
//! (pi process + conversation state), `i18n`.

extern crate gpui_kit as gpui;

pub mod app;
pub mod i18n;
