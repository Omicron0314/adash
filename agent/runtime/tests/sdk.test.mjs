// Real pi SDK + pinned extension + real Rust CLI. The model endpoint is a
// deterministic loopback OpenAI-compatible server; no paid model or cloud is used.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createServer } from 'node:http';
import { spawn, execFileSync } from 'node:child_process';
import { createInterface } from 'node:readline';
import { EventEmitter } from 'node:events';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, symlinkSync, rmSync, existsSync, readdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const tool = resolve(process.env.ADASH_TOOL ?? join(root, 'target/debug/adash-tool'));
const runtime = fileURLToPath(new URL('../dist/index.js', import.meta.url));
const tools = ['ucloud_inventory','ucloud_describe_resource','ucloud_projects','ucloud_billing','ucloud_flows','ucloud_flow','ucloud_plan_deploy','ucloud_plan_optimize','ucloud_plan_stop','ucloud_apply_plan'].sort();
class Host {
  lines=[]; log=''; events=new EventEmitter();
  constructor(env) {
    this.child=spawn(process.execPath,[runtime],{env,stdio:['pipe','pipe','pipe']});
    createInterface({input:this.child.stdout}).on('line',line=>{this.lines.push(JSON.parse(line));this.events.emit('change');});
    this.child.stderr.on('data',chunk=>this.log=(this.log+chunk).slice(-65536));
    this.closed=new Promise(resolve=>this.child.once('close',(code)=>{this.exited=true;this.events.emit('change');resolve(code);}));
    this.child.on('error',error=>{this.log+=error.message;this.events.emit('change');});
  }
  send(command) { this.child.stdin.write(JSON.stringify(command)+'\n'); }
  wait(type, from=0, predicate=()=>true) {
    return new Promise((resolve,reject)=>{
      const cleanup=()=>{clearTimeout(timer);this.events.off('change',check);};
      const check=()=>{
        const found=this.lines.slice(from).find(line=>line.type===type&&predicate(line));
        if(found){cleanup();resolve(found);} else if(this.exited){cleanup();reject(new Error(`SDK exited before ${type}: ${this.log}`));}
      };
      const timer=setTimeout(()=>{cleanup();reject(new Error(`SDK timeout: ${type}; events=${JSON.stringify(this.lines.slice(from))}; ${this.log}`));},20000);
      this.events.on('change',check);check();
    });
  }
  async dispose() {
    if(!this.exited){this.send({type:'dispose'});await this.wait('disposed');this.child.stdin.end();}
    const timer=setTimeout(()=>this.child.kill('SIGKILL'),2000);
    try { assert.equal(await this.closed,0,'SDK must exit cleanly after dispose and EOF'); } finally { clearTimeout(timer); }
  }
}

test('SDK startup, tool boundary, confirmation, streaming, abort and model errors', {timeout:90000}, async t=>{
  assert.ok(existsSync(tool),'build the Rust CLI first: cargo build -p adash-tool');
  const directory=mkdtempSync(join(tmpdir(),'adash-sdk-test-'));
  const workspace=join(directory,'workspace');
  const config=join(directory,'config');
  const state=join(directory,'state');
  mkdirSync(join(workspace,'agent/extensions'),{recursive:true});
  mkdirSync(join(workspace,'agent/skills'),{recursive:true});
  mkdirSync(join(workspace,'.pi/extensions'),{recursive:true});
  mkdirSync(config,{recursive:true});
  symlinkSync(join(root,'agent/extensions/ucloud'),join(workspace,'agent/extensions/ucloud'));
  writeFileSync(join(workspace,'.pi/extensions/forbidden.ts'),'throw new Error("FORBIDDEN_PROJECT_EXTENSION");');
  writeFileSync(join(workspace,'AGENTS.md'),'PRIVATE_CONTEXT_MUST_NOT_APPEAR');
  const requests=[];
  const server=createServer(async(req,res)=>{
    let raw='';for await(const chunk of req)raw+=chunk;
    const body=JSON.parse(raw);requests.push(body);
    const messages=body.messages;
    const lastUser=messages.findLastIndex(message=>message.role==='user');
    const text=content=>typeof content==='string'?content:(content??[]).filter(part=>part.type==='text').map(part=>part.text).join('\n');
    const scenario=text(messages[lastUser].content);
    const results=messages.slice(lastUser+1).filter(message=>message.role==='tool');
    if(scenario==='failure'){res.writeHead(400,{'Content-Type':'application/json'});res.end(JSON.stringify({error:{message:'local model failure',type:'invalid_request_error'}}));return;}
    res.writeHead(200,{'Content-Type':'text/event-stream'});
    const chunk=(delta,finish=null)=>res.write(`data: ${JSON.stringify({id:'local-test',object:'chat.completion.chunk',created:1,model:body.model,choices:[{index:0,delta,finish_reason:finish}]})}\n\n`);
    const call=(name,args)=>{chunk({role:'assistant',tool_calls:[{index:0,id:`call-${requests.length}`,type:'function',function:{name,arguments:JSON.stringify(args)}}]});chunk({},'tool_calls');res.end('data: [DONE]\n\n');};
    if(scenario==='hold'){chunk({role:'assistant',content:'运行中'});return;}
    if(scenario==='inventory'&&results.length===0){call('ucloud_inventory',{});return;}
    if(['reject','approve','without-token','cancel-confirmation'].includes(scenario)){
      if(results.length===0){call('ucloud_plan_deploy',{project:`sdk-${scenario}`,count:1});return;}
      if(results.length===1){const plan=text(results[0].content).match(/plan_id: (\S+)/);assert.ok(plan,'tool returned a plan ID');call('ucloud_apply_plan',{plan_id:plan[1]});return;}
    }
    chunk({role:'assistant',content:'SDK '});chunk({content:'完成'});chunk({},'stop');res.end('data: [DONE]\n\n');
  });
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  writeFileSync(join(config,'models.json'),JSON.stringify({providers:{'adash-test':{baseUrl:`http://127.0.0.1:${server.address().port}/v1`,api:'openai-completions',apiKey:'LOCAL_TEST_KEY',models:[{id:'nested/model',contextWindow:32768,maxTokens:1024,reasoning:false,input:['text']}]}}}));
  const env={...Object.fromEntries(Object.entries(process.env).filter(([key])=>!key.startsWith('ADASH_'))),ADASH_ROOT:workspace,ADASH_TOOL:tool,ADASH_DATA_DIR:state,ADASH_MODEL:'adash-test/nested/model',PI_CODING_AGENT_DIR:config,UDASH_MOCK:'1',UCLOUD_PRIVATE_KEY:'UCLOUD_PRIVATE_SENTINEL'};
  const host=new Host(env);
  t.after(async()=>{await host.dispose();server.closeAllConnections();await new Promise(resolve=>server.close(resolve));rmSync(directory,{recursive:true,force:true});});
  const store=()=>JSON.parse(readFileSync(join(state,'state.json'),'utf8'));
  await t.test('prompt waits for startup; only the ten cloud tools are active',async()=>{
    host.send({type:'start'});host.send({type:'prompt',message:'inventory'});
    const ready=await host.wait('ready');assert.equal(ready.provider,'adash-test');assert.equal(ready.model,'nested/model');assert.deepEqual(ready.tools.sort(),tools);
    await host.wait('agent_settled');
    assert.ok(host.lines.filter(line=>line.type==='message_update').length>=2);
    assert.ok(host.lines.some(line=>line.type==='tool_execution_end'&&line.toolName==='ucloud_inventory'&&!line.isError));
    assert.deepEqual(requests[0].tools.map(tool=>tool.function.name).sort(),tools);
    assert.ok(!JSON.stringify(requests).includes('PRIVATE_CONTEXT_MUST_NOT_APPEAR'));
  });
  for(const scenario of ['reject','without-token','approve','cancel-confirmation']){
    await t.test(`${scenario}: confirmation is correlated and cannot mint approval`,async()=>{
      const from=host.lines.length;host.send({type:'prompt',message:scenario});
      const confirmation=await host.wait('ui_request',from);
      assert.equal(confirmation.title,'adash:apply-plan');
      const before=store().runs.length;
      if(scenario==='approve')execFileSync(tool,['approve',confirmation.message],{env,stdio:['ignore','pipe','pipe']});
      if(scenario==='cancel-confirmation')host.send({type:'abort'});
      else host.send({type:'ui_response',id:confirmation.id,confirmed:scenario!=='reject'});
      await host.wait('agent_settled',from);
      assert.equal(store().runs.length,before+(scenario==='approve'?1:0));
      assert.deepEqual(store().approvals,{});
      if(scenario==='without-token')assert.ok(host.lines.slice(from).some(line=>line.type==='tool_execution_end'&&line.toolName==='ucloud_apply_plan'&&line.isError));
      host.send({type:'ui_response',id:confirmation.id,confirmed:true}); // replay is ignored
    });
  }
  await t.test('abort closes an active model stream and permits the next prompt',async()=>{
    const from=host.lines.length;host.send({type:'prompt',message:'hold'});
    await host.wait('message_update',from);host.send({type:'abort'});await host.wait('agent_settled',from);
    const next=host.lines.length;host.send({type:'prompt',message:'inventory'});await host.wait('agent_settled',next);
  });
  await t.test('provider errors settle and reach the GUI error channel',async()=>{
    const from=host.lines.length;host.send({type:'prompt',message:'failure'});
    const error=await host.wait('error',from);assert.match(error.error,/local model failure/);await host.wait('agent_settled',from);
    assert.ok(!JSON.stringify(host.lines).includes('LOCAL_TEST_KEY'));
  });
  await t.test('startup failure rejects queued prompts and leaves no active session',async()=>{
    const failed=new Host({...env,ADASH_MODEL:'adash-test/missing/nested-model'});
    try {
      failed.send({type:'start'});failed.send({type:'prompt',message:'inventory'});
      assert.match((await failed.wait('startup_error')).error,/missing\/nested-model/);
      await failed.wait('error');failed.send({type:'health'});
      assert.equal((await failed.wait('health')).started,false);
    } finally { await failed.dispose(); }
  });
  await t.test('cloud secrets stay out of model requests, UI events and session files',()=>{
    assert.ok(!JSON.stringify(requests).includes('UCLOUD_PRIVATE_SENTINEL'));
    assert.ok(!JSON.stringify(host.lines).includes('UCLOUD_PRIVATE_SENTINEL'));
    const check=directory=>{for(const entry of readdirSync(directory,{withFileTypes:true})){
      const path=join(directory,entry.name);
      if(entry.isDirectory())check(path);
      else { const text=readFileSync(path,'utf8'); assert.ok(!text.includes('UCLOUD_PRIVATE_SENTINEL')); assert.ok(!text.includes('LOCAL_TEST_KEY')); }
    }};
    check(state);
  });
  await t.test('set_model preserves nested IDs',async()=>{
    const from=host.lines.length;host.send({type:'set_model',model:'adash-test/nested/model'});
    assert.equal((await host.wait('model',from)).model,'adash-test/nested/model');
  });
});
