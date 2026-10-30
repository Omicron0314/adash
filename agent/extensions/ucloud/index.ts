/**
 * adash UCloud tools for pi.
 *
 * Every tool shells out to `adash-tool` (Rust). This file never talks to the
 * UCloud API and never touches credentials. The only mutating tool is
 * `ucloud_apply_plan`, which requires a GUI confirmation (extension UI
 * `confirm`, title `adash:apply-plan`, message = plan id). The app renders the
 * plan card and, when the user clicks confirm, writes the one-time approval the
 * tool consumes. A terminal-only session asks the same question through
 * `ctx.ui.confirm` and then mints the approval itself.
 *
 * See AGENTS.md §3 and docs/ARCHITECTURE.md.
 */

import { spawn } from "node:child_process";
import { Type } from "@earendil-works/pi-ai";
import { defineTool, type ExtensionAPI } from "@earendil-works/pi-coding-agent";

const ADASH_TOOL = process.env.ADASH_TOOL ?? "adash-tool";

interface ToolEnvelope {
  ok: boolean;
  data?: Record<string, unknown>;
  error?: { code?: string; message?: string };
}

interface ToolFailure extends Error {
  code?: string;
}

function runTool(command: string, input?: unknown, args: string[] = [], signal?: AbortSignal): Promise<ToolEnvelope> {
  return new Promise((resolve, reject) => {
    const child = spawn(ADASH_TOOL, [command, ...args], {
      signal,
      stdio: ["pipe", "pipe", "pipe"],
    });
    let out = "";
    let err = "";
    child.stdout.on("data", (chunk) => (out += chunk));
    child.stderr.on("data", (chunk) => (err += chunk));
    child.on("error", (error) =>
      reject(new Error(`无法启动 adash-tool（${ADASH_TOOL}）：${error.message}`)),
    );
    child.on("close", (code) => {
      let parsed: ToolEnvelope | undefined;
      try {
        parsed = JSON.parse(out) as ToolEnvelope;
      } catch {
        reject(new Error(`adash-tool ${command} 输出无效（exit ${code}）：${err.trim() || out}`));
        return;
      }
      if (code !== 0 || parsed.ok === false) {
        const failure = new Error(
          parsed.error?.message ?? err.trim() ?? `adash-tool ${command} exited ${code}`,
        ) as ToolFailure;
        failure.code = parsed.error?.code;
        reject(failure);
        return;
      }
      resolve(parsed);
    });
    child.stdin.end(JSON.stringify(input ?? {}));
  });
}

function result(envelope: ToolEnvelope, summary?: string, extra?: Record<string, unknown>) {
  const text = summary ?? JSON.stringify(envelope.data ?? {}, null, 1);
  return {
    content: [{ type: "text" as const, text }],
    details: { ...(envelope.data ?? {}), ...(extra ?? {}) },
  };
}

const READ = { readOnlyHint: true, destructiveHint: false, openWorldHint: true };
const PLAN = { readOnlyHint: true, destructiveHint: false, openWorldHint: false };

function readTool(name: string, command: string, description: string, parameters: object) {
  return defineTool({
    name,
    label: name,
    description,
    parameters: parameters as never,
    annotations: READ,
    async execute(_id: string, params: unknown, signal?: AbortSignal) {
      const envelope = await runTool(command, params, [], signal);
      return result(envelope);
    },
  });
}

function planTool(name: string, command: string, description: string, parameters: object) {
  return defineTool({
    name,
    label: name,
    description: `${description} 只生成计划，不会改变云上资源；必须由 ucloud_apply_plan 经用户确认后才执行。`,
    parameters: parameters as never,
    annotations: PLAN,
    async execute(_id: string, params: unknown, signal?: AbortSignal) {
      const envelope = await runTool(command, params, [], signal);
      const plan = (envelope.data ?? {}) as {
        plan_id?: string;
        monthly_delta?: string;
        plan?: { summary?: string; reversible?: boolean; risks?: string[] };
      };
      const summary = [
        plan.plan?.summary ?? "计划已生成",
        `净月成本 ${plan.monthly_delta ?? "¥0.00"}`,
        plan.plan?.reversible === false ? "不可逆，需二次确认" : "可逆",
        `plan_id: ${plan.plan_id ?? "(缺失)"}`,
        ...(plan.plan?.risks ?? []).map((risk) => `风险：${risk}`),
      ].join("\n");
      return result(envelope, summary, { plan_id: plan.plan_id });
    },
  });
}

export default function (pi: ExtensionAPI) {
  pi.registerTool(
    readTool(
      "ucloud_inventory",
      "inventory",
      "列出账户里的全部云资源摘要：产品、地域、状态、所属项目、月成本。用户问「我有哪些资源」时先调它。",
      Type.Object({}),
    ),
  );
  pi.registerTool(
    readTool(
      "ucloud_describe_resource",
      "resource",
      "查看单个资源的完整字段（规格、IP、绑定关系、创建时间）。",
      Type.Object({
        id: Type.String({ description: "资源 id，形如 uhost:uhost-shopweb1" }),
      }),
    ),
  );
  pi.registerTool(
    readTool("ucloud_projects", "projects", "列出项目、资源数、本月花费与预算。", Type.Object({})),
  );
  pi.registerTool(
    readTool("ucloud_billing", "billing", "查询余额、本月花费，以及按项目/产品分摊。", Type.Object({})),
  );
  pi.registerTool(
    readTool(
      "ucloud_flows",
      "flows",
      "列出每个项目的流程图概况（节点数、连线数、执行次数）。",
      Type.Object({}),
    ),
  );
  pi.registerTool(
    readTool(
      "ucloud_flow",
      "flow",
      "查看某个项目的完整流程图：节点、连线及其证据、执行记录。用户问「这个项目用了哪些服务」时用它。",
      Type.Object({ project: Type.String({ description: "项目 slug，例如 shop-api" }) }),
    ),
  );

  pi.registerTool(
    planTool(
      "ucloud_plan_deploy",
      "plan-deploy",
      "为项目生成部署计划：指定地域、规格与数量，返回带月成本的计划卡片。",
      Type.Object({
        project: Type.String({ description: "项目 slug（小写字母、数字、短横线）" }),
        region: Type.Optional(Type.String({ description: "地域，例如 cn-bj2；默认 cn-bj2" })),
        spec: Type.Optional(Type.String({ description: "主机规格，例如 2C / 4GiB" })),
        count: Type.Optional(Type.Number({ description: "主机数量，默认 1" })),
        repo: Type.Optional(Type.String({ description: "代码仓库，写入流程图的外部节点" })),
      }),
    ),
  );
  pi.registerTool(
    planTool(
      "ucloud_plan_optimize",
      "plan-optimize",
      "运行省钱规则（孤儿 EIP、未挂载云盘、停机仍计费的主机），生成带金额的节省计划。",
      Type.Object({}),
    ),
  );
  pi.registerTool(
    planTool(
      "ucloud_plan_stop",
      "plan-stop",
      "生成停机计划：保留公网 IP 与云盘，不做删除。",
      Type.Object({
        resource: Type.String({ description: "资源 id，形如 uhost:uhost-shopweb1" }),
      }),
    ),
  );

  pi.registerTool(
    defineTool({
      name: "ucloud_apply_plan",
      label: "Apply plan",
      description:
        "执行一个由 ucloud_plan_* 生成、且已在应用计划卡片中经用户确认的计划。绝不自行构造 plan_id。",
      parameters: Type.Object({
        plan_id: Type.String({ description: "ucloud_plan_* 返回的 plan_id" }),
      }),
      annotations: { readOnlyHint: false, destructiveHint: true, openWorldHint: true },
      async execute(
        _id: string,
        params: { plan_id: string },
        signal?: AbortSignal,
        _onUpdate?: unknown,
        ctx?: {
          ui: { confirm(title: string, message: string, opts?: object): Promise<boolean> };
          mode?: string;
        },
      ) {
        const confirmed = await ctx!.ui.confirm("adash:apply-plan", params.plan_id, {
          timeout: 5 * 60 * 1000,
        });
        if (!confirmed) {
          return {
            content: [{ type: "text" as const, text: "用户拒绝了该计划，云上没有任何变更。" }],
            details: { ok: false, rejected: true, plan_id: params.plan_id },
          };
        }

        // The app writes the approval while rendering the plan card. A terminal
        // session has no card, so it mints the approval after the same confirm.
        const terminal = ctx!.mode === "tui" || process.env.ADASH_TERMINAL_APPROVAL === "1";
        if (terminal) {
          await runTool("approve", {}, [params.plan_id], signal);
        }

        try {
          const applied = await runTool("apply", {}, [params.plan_id], signal);
          const data = (applied.data ?? {}) as Record<string, unknown>;
          const summary = [
            String(data.note ?? "计划已执行。"),
            data.project ? `项目：${String(data.project)}` : undefined,
          ]
            .filter(Boolean)
            .join("\n");
          return result(applied, summary, { plan_id: params.plan_id });
        } catch (error) {
          const failure = error as ToolFailure;
          if (failure.code === "approval") {
            throw new Error(
              "计划还没有在应用里确认（或批准已过期）。请在计划卡片中点击确认后重试。",
            );
          }
          throw error;
        }
      },
    }),
  );
}
