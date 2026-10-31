import { ModelRuntime } from '@earendil-works/pi-coding-agent';
import { InMemoryCredentialStore } from '@earendil-works/pi-ai';
import { join } from 'node:path';

export type ProviderSettings = { endpoint: string; api_key: string; model: string; api: string; context_window: number; max_tokens: number };
export const providerId = 'adash-configured';
export async function configuredRuntime(value: unknown, directory: string) {
  const config = value as Partial<ProviderSettings> | undefined;
  if (!config || typeof config.endpoint !== 'string' || typeof config.model !== 'string' || !config.model.trim()
    || typeof config.api_key !== 'string' || !config.api_key.trim()) throw new Error('请在设置 → AI 中填写 Endpoint、API key 和模型');
  if (/[\u0000-\u001f\u007f]/u.test(config.api_key)) throw new Error('API key 不能包含控制字符');
  let endpoint: URL;
  try { endpoint = new URL(config.endpoint); } catch { throw new Error('AI Endpoint 无效'); }
  if (!['https:', 'http:'].includes(endpoint.protocol) || endpoint.username || endpoint.password || endpoint.search || endpoint.hash) throw new Error('AI Endpoint 必须是无凭据的 HTTP/HTTPS Base URL');
  const api = config.api ?? 'openai-completions';
  if (api !== 'openai-completions' && api !== 'openai-responses' && api !== 'anthropic-messages') throw new Error('不支持的 AI API 协议');
  const contextWindow = config.context_window ?? 32768;
  const maxTokens = config.max_tokens ?? 4096;
  if (!Number.isSafeInteger(contextWindow) || !Number.isSafeInteger(maxTokens) || maxTokens <= 0 || contextWindow <= maxTokens) throw new Error('AI 上下文与输出长度无效');
  const runtime = await ModelRuntime.create({
    credentials: new InMemoryCredentialStore(), modelsPath: null, modelsStorePath: join(directory, 'catalog.json'),
    allowModelNetwork: false, refreshOnCreate: false,
  });
  runtime.registerProvider(providerId, {
    baseUrl: config.endpoint.replace(/\/+$/, ''), api,
    models: [{ id: config.model, name: config.model, reasoning: false, input: ['text'],
      contextWindow, maxTokens, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 } }],
  });
  // Runtime credentials are literal, in-memory values, never models.json
  // interpolation/commands and never the user's global pi credentials.
  await runtime.setRuntimeApiKey(providerId, config.api_key);
  const model = runtime.getModel(providerId, config.model);
  if (!model) throw new Error('AI 模型配置失败');
  return { runtime, model };
}
