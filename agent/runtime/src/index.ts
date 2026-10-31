/**
 * adash's embedded pi SDK host.
 *
 * Rust starts this once. Each line on stdin is a permissioned command; each line
 * on stdout is one JSON event. The SDK owns the conversation, model, tools,
 * session and extension runtime. Model/network I/O belongs to the SDK; this
 * host itself only speaks framed JSONL and never exposes credentials or
 * session files to the GUI.
 */
import { parseModelChoice, projectEvent } from "./protocol.js";
import { createInterface } from "node:readline";
import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  createAgentSession,
  DefaultResourceLoader,
  ModelRuntime,
  SessionManager,
  SettingsManager,
  type AgentSession,
  type ExtensionUIContext,
} from "@earendil-works/pi-coding-agent";

const root = resolve(process.env.ADASH_ROOT ?? join(dirname(fileURLToPath(import.meta.url)), "../../.."));
const tool = process.env.ADASH_TOOL ?? join(root, "target/debug/adash-tool");
const extension = join(root, "agent/extensions/ucloud/index.ts");
const skills = join(root, "agent/skills");
const systemPromptFile = join(root, "agent/prompts/system.md");
const modelId = process.env.ADASH_MODEL;
type Command = Record<string, unknown>;
let session: AgentSession | undefined;
let unsubscribe: (() => void) | undefined;
let lastText = "";
let starting: Promise<void> | undefined;
let confirmId = 0;
const confirmations = new Map<string, (confirmed: boolean) => void>();
const emit = (value: unknown) => process.stdout.write(JSON.stringify(value) + "\n");
const imagePath = (name: string) => join(root, "assets", name);
const verify: (condition: unknown, message: string) => asserts condition = (condition, message) => {
  if (!condition) throw new Error(message);
};

function agentDir() {
  return process.env.ADASH_AGENT_DIR ?? join(process.env.XDG_DATA_HOME ?? join(process.env.HOME ?? ".", ".local", "share"), "adash", "agent");
}

function uiBindings(): ExtensionUIContext {
  // Only the dialogs extensions actually use are meaningful here; the rest are
  // no-ops because there is no terminal. A confirmation is never auto-accepted.
  const context = {
    confirm: (title: string, message: string) => new Promise<boolean>((resolve) => {
      const id = `confirm-${++confirmId}`;
      const timer = setTimeout(() => finish(false), 300_000);
      timer.unref();
      const finish = (confirmed: boolean) => {
        clearTimeout(timer);
        confirmations.delete(id);
        resolve(confirmed);
      };
      confirmations.set(id, finish);
      emit({ type: "ui_request", id, method: "confirm", title, message });
    }),
    select: async () => undefined,
    input: async () => undefined,
    notify: (message: string) => emit({ type: "notice", message }),
    setStatus: (key: string, text: string) => emit({ type: "status", key, text }),
    setTitle: () => {},
    editor: async () => undefined,
    onTerminalInput: () => () => {},
    setWorkingMessage: () => {},
    setWorkingVisible: () => {},
    setWorkingIndicator: () => {},
    setHiddenThinkingLabel: () => {},
    setWidget: () => {},
    setFooter: () => {},
    setHeader: () => {},
    custom: async () => undefined,
    pasteToEditor: () => {},
    setEditorText: () => {},
    getEditorText: () => "",
    addAutocompleteProvider: () => {},
    setEditorComponent: () => {},
    getEditorComponent: () => undefined,
    getAllThemes: () => [],
    getTheme: () => undefined,
    setTheme: () => ({ success: false }),
    theme,
  };
  return context as unknown as ExtensionUIContext;
}

type StartCommand = { model?: string; session_dir?: string };

// The SDK context is used only for confirmation/status notifications; its
// terminal-renderer theme is never used by this GPUI application.
const theme = {
  name: "adash", colors: {
    text: "#1c1b22", muted: "#47464f", dim: "#787680", accent: "#4b49c8",
    border: "#c8c5d0", success: "#246b3a", warning: "#7a5900", error: "#ba1a1a",
    background: "#f9f6fe", surface: "#ffffff", surfaceAlt: "#f0ecf6",
    user: "#2c2891", tool: "#47464f",
  },
};

async function start(command: StartCommand) {
  const modelRuntime = await ModelRuntime.create({ allowModelNetwork: false });
  const settings = SettingsManager.inMemory({ compaction: { enabled: true }, retry: { enabled: true, maxRetries: 2 } });
  const loader = new DefaultResourceLoader({
    cwd: root,
    agentDir: agentDir(),
    // Standard discovery is off; the pinned extension and repo skills are explicit.
    noExtensions: true,
    noSkills: true,
    noContextFiles: true,
    noPromptTemplates: true,
    noThemes: true,
    settingsManager: settings,
    additionalExtensionPaths: [extension],
    additionalSkillPaths: [skills],
    appendSystemPrompt: existsSync(systemPromptFile) ? [await (await import("node:fs/promises")).readFile(systemPromptFile, "utf8")] : [],
  });
  await loader.reload();
  const choices = command.model || modelId;
  let model;
  if (choices) {
    const { provider, id } = parseModelChoice(choices);
    model = provider ? modelRuntime.getModel(provider, id) : modelRuntime.getAvailableSnapshot().find((entry) => entry.id === id);
    verify(model, `模型不可用：${choices}`);
  }
  const created = await createAgentSession({
    cwd: root,
    agentDir: agentDir(),
    modelRuntime,
    settingsManager: settings,
    resourceLoader: loader,
    model,
    sessionManager: SessionManager.create(root, command.session_dir),
    // No shell or edit tools. The extension is the only action surface.
    tools: ["ucloud_inventory", "ucloud_describe_resource", "ucloud_projects", "ucloud_billing", "ucloud_flows", "ucloud_flow", "ucloud_plan_deploy", "ucloud_plan_optimize", "ucloud_plan_stop", "ucloud_apply_plan"],
  });
  session = created.session;
  unsubscribe?.();
  unsubscribe = session.subscribe((event) => {
    if (event.type === "message_end") { lastText = session?.getLastAssistantText() ?? lastText; }
    const projected = projectEvent(event);
    if (projected) emit(projected);
  });
  await session.bindExtensions({ uiContext: uiBindings(), mode: "rpc" });
  emit({ type: "ready", provider: session.model?.provider ?? "", model: session.model?.id ?? "", tools: session.getActiveToolNames?.() ?? [] });
}

async function sendPrompt(message: string, images?: never[]) {
  verify(session, "agent is not started");
  try {
    await session.prompt(message, images ? { images } : undefined);
  } catch (error) {
    emit({ type: "error", error: error instanceof Error ? error.message : String(error) });
  }
}

const handlers: Record<string, (command: Command) => unknown> = {
  async start(command: Command) {
    starting = start(command);
    try { await starting; } catch (error) {
      emit({ type: "startup_error", error: error instanceof Error ? error.message : String(error) });
    }
  },
  async prompt(command) {
    await starting;
    if (!session) { emit({ type: "error", error: "agent is not started" }); return; }
    void sendPrompt(String(command.message ?? ""));
  },
  ui_response(command) {
    confirmations.get(String(command.id))?.(command.confirmed === true);
  },
  steer(command: Command) { session?.steer(String(command.message ?? "")).catch((error: unknown) => emit({ type: "error", error: String(error) })); },
  follow_up(command: Command) { session?.followUp(String(command.message ?? "")).catch((error: unknown) => emit({ type: "error", error: String(error) })); },
  async abort() {
    for (const resolve of [...confirmations.values()]) resolve(false);
    await session?.abort();
  },
  async set_model(command: Command) {
    const { provider, id } = parseModelChoice(String(command.model ?? ""));
    const model = session?.modelRuntime.getModel(provider, id);
    if (model && session) { await session.setModel(model); emit({ type: "model", model: `${provider}/${id}` }); }
  },
  async models() {
    const runtime = await ModelRuntime.create({ allowModelNetwork: false });
    emit({ type: "models", models: (await runtime.getAvailable()).map((entry) => `${entry.provider}/${entry.id}`) });
  },
  async status() { emit({ type: "status_snapshot", text: lastText }); },
  async dispose() {
    for (const resolve of [...confirmations.values()]) resolve(false);
    try { await session?.abort(); } finally { session?.dispose(); session = undefined; }
    emit({ type: "disposed" });
  },
  health() { emit({ type: "health", started: Boolean(session), icon: imagePath("adash.svg") }); },
};

createInterface({ input: process.stdin, terminal: false }).on("line", async (line) => {
  let command;
  try { command = JSON.parse(line); } catch { emit({ type: "error", error: "invalid command" }); return; }
  const handler = handlers[String(command?.type)];
  if (!handler) { emit({ type: "error", error: `unknown command: ${command?.type}` }); return; }
  try { await handler(command); } catch (error) { emit({ type: "error", error: error instanceof Error ? error.message : String(error) }); }
}).on("close", () => { session?.dispose(); });
process.on("SIGTERM", () => { session?.dispose(); process.exit(0); });
process.on("uncaughtException", (error) => emit({ type: "error", error: error.message }));
