import assert from 'node:assert/strict';
import { test } from 'node:test';
import { projectEvent } from '../dist/protocol.js';
import { configuredRuntime } from '../dist/provider.js';

test('user configuration is mandatory and credentials cannot hide in endpoint URLs', async () => {
  await assert.rejects(configuredRuntime(undefined, '.'), /设置.*AI/);
  await assert.rejects(configuredRuntime({endpoint:'https://key@example.org',model:'custom/model',api_key:'test'}, '.'), /Endpoint/);
  await assert.rejects(configuredRuntime({endpoint:'https://example.org?key=secret',model:'custom/model',api_key:'test'}, '.'), /Endpoint/);
});
test('stream only the delta and supported metadata to the GUI', () => {
  const event = {type:'message_update', assistantMessageEvent:{type:'text_delta',delta:'中文'},message:{privateProviderData:'secret',content:'repeated full message'}};
  assert.deepEqual(projectEvent(event), {type:'message_update',assistantMessageEvent:{type:'text_delta',delta:'中文'}});
  assert.deepEqual(projectEvent({type:'tool_execution_end',toolName:'ucloud_inventory',isError:false,result:{privateProviderData:'secret'}}), {type:'tool_execution_end',toolName:'ucloud_inventory',isError:false,durationMs:undefined});
  assert.equal(projectEvent({type:'message_start',message:{privateProviderData:'secret'}}), undefined);
  assert.deepEqual(projectEvent({type:'message_end',message:{role:'assistant',stopReason:'error',errorMessage:'model failed'}}), {type:'error',error:'model failed'});
});
