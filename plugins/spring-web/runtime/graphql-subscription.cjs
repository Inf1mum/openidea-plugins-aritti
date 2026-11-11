'use strict';
// Independent bounded graphql-transport-ws client. Configuration enters stdin,
// never argv, disk or logs. No application files or dependencies are loaded.
const readline = require('node:readline');
const major = Number(process.versions.node.split('.')[0]);
if (process.argv.includes('--probe')) {
  if (major < 22 || typeof WebSocket !== 'function') process.exit(2);
  process.stdout.write(JSON.stringify({protocol:'openidea-graphql-ws-v1',node:process.versions.node})+'\n');
  process.exit(0);
}
if (major < 22 || typeof WebSocket !== 'function') process.exit(2);
const LIMIT = 256 * 1024;
let bytes = 0, socket, active = false, finished = false, configured = false, acked = false;
let ackTimer, lifeTimer, exitTimer;
function object(v) {return v !== null && typeof v === 'object' && !Array.isArray(v);}
function output(type, value) {
  const line = JSON.stringify({type,...(value === undefined ? {} : {payload:value})})+'\n';
  if (bytes + Buffer.byteLength(line) > (type === 'next' ? LIMIT-1024 : LIMIT)) return false;
  bytes += Buffer.byteLength(line); process.stdout.write(line); return true;
}
function finish(type, code, payload, sendComplete = false) {
  if (finished) return;
  finished = true; clearTimeout(ackTimer); clearTimeout(lifeTimer);
  if (sendComplete && active && socket?.readyState === WebSocket.OPEN) {
    try {socket.send(JSON.stringify({id:'1',type:'complete'}));} catch {}
  }
  active = false;
  output(type,payload);
  if (socket && socket.readyState === WebSocket.OPEN) {
    try {socket.close(1000);} catch {}
  } else if (socket && socket.readyState === WebSocket.CONNECTING) {
    try {socket.close();} catch {}
  }
  // Give Complete/close and stdout a bounded opportunity to flush.
  exitTimer = setTimeout(() => process.exit(code),500);
}
function fail(message) {finish('failure',1,message,true);}
function validEndpoint(endpoint) {
  if (typeof endpoint !== 'string' || endpoint.length > 2048 || /[\s\\\u0000-\u001f\u007f]/u.test(endpoint)) return false;
  try {
    const url = new URL(endpoint);
    return ['ws:','wss:'].includes(url.protocol) && !url.username && !url.password && !url.hash && url.pathname.startsWith('/') && url.href === endpoint;
  } catch {return false;}
}
function start(config) {
  if (!object(config) || !validEndpoint(config.endpoint) || typeof config.query !== 'string' || Buffer.byteLength(config.query)>8192 || !object(config.variables) || !object(config.init) || (config.operationName !== null && typeof config.operationName !== 'string')) return fail('Invalid reviewed client configuration.');
  configured = true;
  // The WebSocket implementation performs normal TLS validation. No custom
  // headers, cookies, alternate dispatcher or application credentials are read.
  try {socket = new WebSocket(config.endpoint,'graphql-transport-ws');} catch {return fail('WebSocket construction failed.');}
  lifeTimer = setTimeout(() => finish('lifetime-ended',0,'Finite120-second session ended.',true),120_000);
  ackTimer = setTimeout(() => fail('Connection acknowledgement timed out.'),10_000);
  socket.addEventListener('open',() => {
    if (finished) return;
    if (socket.protocol !== 'graphql-transport-ws') return fail('Server did not select graphql-transport-ws.');
    output('socket-open'); socket.send(JSON.stringify({type:'connection_init',payload:config.init}));
  });
  socket.addEventListener('message',event => {
    if (finished) return;
    if (typeof event.data !== 'string' || Buffer.byteLength(event.data)>64*1024) return fail('Binary or oversized protocol message refused.');
    let message; try {message=JSON.parse(event.data);} catch {return fail('Server sent invalid JSON.');}
    if (!object(message) || typeof message.type !== 'string') return fail('Server sent an invalid protocol message.');
    switch (message.type) {
      case 'connection_ack':
        if (acked || 'id' in message || (message.payload != null && !object(message.payload))) return fail('Invalid or duplicate connection acknowledgement.');
        acked=true;clearTimeout(ackTimer);active=true;output('connection-acknowledged');
        socket.send(JSON.stringify({id:'1',type:'subscribe',payload:{query:config.query,variables:config.variables,...(config.operationName===null?{}:{operationName:config.operationName})}}));
        // Retire authentication payload immediately after its one transmission.
        config.init=null; return;
      case 'ping':
      case 'pong':
        if ('id' in message || (message.payload != null && !object(message.payload))) return fail('Invalid heartbeat message.');
        if (message.type==='ping') socket.send(JSON.stringify({type:'pong',...('payload' in message?{payload:message.payload}:{})}));
        return;
      case 'next':
      case 'error':
      case 'complete':
        if (typeof message.id !== 'string') return fail('Missing operation identity.');
        if (message.id !== '1') return; // This adapter owns one operation only.
        if (!acked || !active) return fail('Operation message preceded acknowledgement.');
        if (message.type==='next') {
          if (!object(message.payload)) return fail('Invalid execution result.');
          if (!output('next',message.payload)) return finish('output-limit',1,'Display limit reached; subscription completed.',true);
        } else if (message.type==='error') {
          if (!Array.isArray(message.payload)) return fail('Invalid operation error.');
          finish('operation-error',1,message.payload);
        } else {
          if ('payload' in message) return fail('Invalid completion message.');
          finish('server-complete',0);
        }
        return;
      default: return fail('Unsupported server protocol message.');
    }
  });
  socket.addEventListener('error',() => {if (!finished) fail('WebSocket connection failed.');});
  socket.addEventListener('close',event => {
    if (!finished) finish('connection-closed',1,{code:event.code});
  });
}
let inputBytes=0;
const lines=readline.createInterface({input:process.stdin,crlfDelay:Infinity});
// readline's allocation is bounded by an independent incoming byte counter.
process.stdin.on('data',chunk => {inputBytes+=chunk.length;if(inputBytes>24*1024) fail('Input limit exceeded.');});
lines.on('line',line => {
  if (finished) return;
  let value;try {value=JSON.parse(line);} catch {return fail('Invalid input JSON.');}
  if (!configured) start(value);
  else if (object(value) && value.type==='cancel' && Object.keys(value).length===1) finish('client-cancelled',0,undefined,true);
  else fail('Unexpected client control.');
});
lines.on('close',() => {if (!finished) finish('client-input-closed',0,undefined,true);});
setTimeout(() => {if (!configured&&!finished) fail('Configuration timed out.');},10_000).unref();
