// The GUI consumes a small projection of SDK events. In particular, do not
// serialize full messages on every token or forward provider metadata/secrets.
export function parseModelChoice(choice: string): { provider: string; id: string } {
  const separator = choice.indexOf("/");
  return separator < 0 ? { provider: "", id: choice }
    : { provider: choice.slice(0, separator), id: choice.slice(separator + 1) };
}
const object = (value: unknown): Record<string, unknown> => value !== null && typeof value === "object" ? value as Record<string, unknown> : {};

export function projectEvent(value: unknown): Record<string, unknown> | undefined {
  const event = object(value);
  switch (event.type) {
    case "message_update": {
      const delta = object(event.assistantMessageEvent);
      if (delta.type === "text_delta" && typeof delta.delta === "string") {
        return { type: "message_update", assistantMessageEvent: { type: "text_delta", delta: delta.delta } };
      }
      return undefined;
    }
    case "message_end": {
      const message = object(event.message);
      if (message.role === "assistant" && message.stopReason === "error") {
        return { type: "error", error: typeof message.errorMessage === "string" ? message.errorMessage : "模型返回错误" };
      }
      return undefined;
    }
    case "tool_execution_start": return { type: event.type, toolName: event.toolName };
    case "tool_execution_end": return { type: event.type, toolName: event.toolName, isError: event.isError, durationMs: event.durationMs };
    case "agent_start":
    case "agent_settled": return { type: event.type };
    default: return undefined;
  }
}
