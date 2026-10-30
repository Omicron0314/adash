/**
 * adash pi extension — registers `ucloud_*` tools.
 *
 * Every tool forwards to `adash-tool <command>` (JSON in on stdin, JSON out on
 * stdout); this file never talks to UCloud itself and never implements signing.
 * Mutating tools only ever call `apply <plan_id>` and must go through
 * `ctx.ui.confirm` first. See docs/ARCHITECTURE.md.
 */

export const TOOL_PREFIX = "ucloud_";

/** Side-effect class of a tool; `mutate` requires a GUI confirmation. */
export type SideEffect = "read" | "plan" | "mutate";

export default function register(): void {
  // Tool registration lands in M3; see docs/ROADMAP.md.
}
