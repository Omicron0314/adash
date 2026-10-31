import assert from 'node:assert/strict';
import { test } from 'node:test';
import { parseModelChoice, projectEvent } from '../dist/protocol.js';

test('provider selection preserves nested model IDs', () => {
  assert.deepEqual(parseModelChoice('magpie/auto/deepseek-v4.1-flash'), {provider:'magpie',id:'auto/deepseek-v4.1-flash'});
  assert.deepEqual(parseModelChoice('standalone'), {provider:'',id:'standalone'});
});
test('stream only the delta and supported metadata to the GUI', () => {
  const event = {type:'message_update', assistantMessageEvent:{type:'text_delta',delta:'中文'},message:{privateProviderData:'secret',content:'repeated full message'}};
  assert.deepEqual(projectEvent(event), {type:'message_update',assistantMessageEvent:{type:'text_delta',delta:'中文'}});
  assert.deepEqual(projectEvent({type:'tool_execution_end',toolName:'ucloud_inventory',isError:false,result:{privateProviderData:'secret'}}), {type:'tool_execution_end',toolName:'ucloud_inventory',isError:false,durationMs:undefined});
  assert.equal(projectEvent({type:'message_start',message:{privateProviderData:'secret'}}), undefined);
  assert.deepEqual(projectEvent({type:'message_end',message:{role:'assistant',stopReason:'error',errorMessage:'model failed'}}), {type:'error',error:'model failed'});
});
