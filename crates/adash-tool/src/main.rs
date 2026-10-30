//! `adash-tool <command>` — one JSON in on stdin, one JSON out on stdout.
//!
//! This binary is the agent's only execution surface. Every command is
//! classified `read` / `plan` / `mutate`; `mutate` refuses to run without a
//! one-time approval token written by the GUI. See docs/ARCHITECTURE.md.

fn main() {
    eprintln!("adash-tool: commands are not implemented yet (M1)");
    std::process::exit(2);
}
