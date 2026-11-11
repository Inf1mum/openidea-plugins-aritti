#!/usr/bin/env python3
"""Disposable stdlib RFC6455 fixture for the packaged Node graphql-transport-ws client.
No application server, external package, credential, database or remote network used.
"""
import argparse, base64, hashlib, json, pathlib, socket, struct, subprocess, threading, time, queue
ROOT=pathlib.Path(__file__).resolve().parents[2]
HELPER=(ROOT/'runtime/graphql-subscription.cjs').read_text()
QUERY='subscription Events($id: ID!) { event(id: $id) { value } }'
INIT={'fixtureToken':'disposable-not-an-application-credential'}
BOOTSTRAP="const a=process.argv.slice(1);const m=a.shift();process.argv=['node',...(m==='probe'?['--probe']:[])];eval(a.join(''))"
def command(probe=False):
    args=['node','--eval',BOOTSTRAP,'--','probe' if probe else 'run']+[HELPER[i:i+1000] for i in range(0,len(HELPER),1000)]
    assert len(args)<128 and all(len(a.encode())<=4096 for a in args)
    return args
class Fixture:
    def __init__(self,scenario):
        self.socket=socket.socket();self.socket.bind(('127.0.0.1',0));self.socket.listen(1);self.socket.settimeout(8)
        self.port=self.socket.getsockname()[1];self.scenario=scenario;self.error=None;self.events=[]
        self.thread=threading.Thread(target=self.run,daemon=True);self.thread.start()
    def exact(self,conn,n):
        out=b''
        while len(out)<n:
            part=conn.recv(n-len(out))
            if not part:raise EOFError('socket closed')
            out+=part
        return out
    def receive(self,conn):
        first,second=self.exact(conn,2);size=second&127
        if size==126:size=struct.unpack('!H',self.exact(conn,2))[0]
        elif size==127:size=struct.unpack('!Q',self.exact(conn,8))[0]
        assert size<32768 and second&128,'bounded masked client frame required'
        mask=self.exact(conn,4);data=self.exact(conn,size);data=bytes(v^mask[i%4] for i,v in enumerate(data))
        if first&15==8:
            self.send(conn,data,8);raise EOFError('close frame')
        assert first&15==1 and first&128,'fixture expects one unfragmented client text frame'
        value=json.loads(data);self.events.append(value['type']);return value
    def send(self,conn,value,opcode=1):
        data=(json.dumps(value,separators=(',',':')).encode() if not isinstance(value,bytes) else value)
        length=len(data);header=bytes([128|opcode,length]) if length<126 else bytes([128|opcode,126])+struct.pack('!H',length) if length<65536 else bytes([128|opcode,127])+struct.pack('!Q',length)
        conn.sendall(header+data)
    def run(self):
        try:
            conn,_=self.socket.accept()
            with conn:
                conn.settimeout(8);head=b''
                while b'\r\n\r\n' not in head:
                    head+=conn.recv(4096);assert len(head)<16384
                lines=head.decode().split('\r\n');assert lines[0].startswith('GET /graphql?fixture=')
                headers={k.lower():v.strip() for k,v in (line.split(':',1) for line in lines[1:] if ':' in line)}
                assert headers['sec-websocket-protocol']=='graphql-transport-ws'
                accept=base64.b64encode(hashlib.sha1((headers['sec-websocket-key']+'258EAFA5-E914-47DA-95CA-C5AB0DC85B11').encode()).digest()).decode()
                conn.sendall(f'HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\nSec-WebSocket-Protocol: graphql-transport-ws\r\n\r\n'.encode())
                init=self.receive(conn);assert init=={'type':'connection_init','payload':INIT}
                if self.scenario=='before-ack':
                    self.send(conn,{'id':'1','type':'next','payload':{'data':{}}})
                    try:self.receive(conn)
                    except EOFError:pass
                    assert 'subscribe' not in self.events;return
                self.send(conn,{'type':'ping','payload':{'fixture':'heartbeat'}})
                assert self.receive(conn)=={'type':'pong','payload':{'fixture':'heartbeat'}}
                self.send(conn,{'type':'connection_ack'})
                subscribe=self.receive(conn);assert subscribe=={'id':'1','type':'subscribe','payload':{'query':QUERY,'variables':{'id':'fixture'},'operationName':'Events'}}
                if self.scenario=='error':self.send(conn,{'id':'1','type':'error','payload':[{'message':'fixture validation error'}]})
                elif self.scenario=='duplicate-ack':self.send(conn,{'type':'connection_ack'});assert self.receive(conn)=={'id':'1','type':'complete'}
                elif self.scenario=='invalid-json':self.send(conn,b'not-json');assert self.receive(conn)=={'id':'1','type':'complete'}
                elif self.scenario=='limit':
                    for _ in range(7):self.send(conn,{'id':'1','type':'next','payload':{'data':{'value':'x'*48000}}})
                    assert self.receive(conn)=={'id':'1','type':'complete'}
                else:
                    self.send(conn,{'id':'1','type':'next','payload':{'data':{'event':{'value':'fixture-value'}}}})
                    if self.scenario=='cancel':assert self.receive(conn)=={'id':'1','type':'complete'}
                    else:self.send(conn,{'id':'1','type':'complete'})
                try:self.receive(conn)
                except EOFError:pass
        except Exception as error:self.error=repr(error)
        finally:self.socket.close()
    def finish(self):
        self.thread.join(9);assert not self.thread.is_alive(),'fixture thread did not end';assert self.error is None,self.error

def run_case(case):
    fixture=Fixture(case)
    config={'endpoint':f'ws://127.0.0.1:{fixture.port}/graphql?fixture={case}','query':QUERY,'operationName':'Events','variables':{'id':'fixture'},'init':INIT}
    p=subprocess.Popen(command(),stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
    p.stdin.write(json.dumps(config)+'\n');p.stdin.flush()
    lines=[];notices=queue.Queue()
    def drain():
        for line in p.stdout:
            lines.append(line);notices.put(line)
    reader=threading.Thread(target=drain,daemon=True);reader.start()
    if case=='cancel':
        while True:
            line=notices.get(timeout=8)
            if json.loads(line)['type']=='next':break
        p.stdin.write('{"type":"cancel"}\n');p.stdin.flush()
    p.wait(timeout=12);reader.join(3);assert not reader.is_alive();out=''.join(lines);err=p.stderr.read();p.stdin.close()
    fixture.finish();assert not err,err;assert INIT['fixtureToken'] not in out,'initialization payload leaked into client logs'
    messages=[json.loads(line) for line in out.splitlines()];types=[m['type'] for m in messages]
    expected={'success':'server-complete','cancel':'client-cancelled','error':'operation-error','before-ack':'failure','duplicate-ack':'failure','invalid-json':'failure','limit':'output-limit'}[case]
    assert expected in types,(case,types)
    assert p.returncode==(0 if case in ('success','cancel') else 1),(case,p.returncode)
    assert len(out.encode())<=256*1024
    return {'case':case,'exit':p.returncode,'types':types,'client_frames':fixture.events,'output_bytes':len(out.encode())}

def redirect_case():
    target=socket.socket();target.bind(('127.0.0.1',0));target.listen(1);target.settimeout(2)
    first=socket.socket();first.bind(('127.0.0.1',0));first.listen(1)
    port=first.getsockname()[1];target_port=target.getsockname()[1]
    def serve():
        conn,_=first.accept()
        with conn:conn.recv(8192);conn.sendall(f'HTTP/1.1 302 Found\r\nLocation: ws://127.0.0.1:{target_port}/stolen\r\nContent-Length: 0\r\nConnection: close\r\n\r\n'.encode())
        first.close()
    thread=threading.Thread(target=serve,daemon=True);thread.start()
    p=subprocess.Popen(command(),stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
    p.stdin.write(json.dumps({'endpoint':f'ws://127.0.0.1:{port}/graphql','query':QUERY,'operationName':'Events','variables':{},'init':INIT})+'\n');p.stdin.flush();p.wait(timeout=8)
    out=p.stdout.read();err=p.stderr.read();p.stdin.close();thread.join(2)
    assert p.returncode==1 and not err and 'failure' in out
    followed=False
    try:conn,_=target.accept();followed=True;conn.close()
    except socket.timeout:pass
    finally:target.close()
    assert not followed,'client followed redirect to an unreviewed authority'
    return {'case':'redirect-refused','exit':p.returncode,'redirect_followed':followed}

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--receipt',required=True);args=parser.parse_args()
    probe=subprocess.run(command(True),capture_output=True,text=True,check=True)
    proof=json.loads(probe.stdout);assert int(proof['node'].split('.')[0])>=22
    results=[run_case(case) for case in ('success','cancel','error','before-ack','duplicate-ack','invalid-json','limit')];results.append(redirect_case())
    receipt={'runtime':proof,'fixture':'stdlib Python disposable loopback RFC6455 server; no remote/application resources','helper_sha256':hashlib.sha256(HELPER.encode()).hexdigest(),'bootstrap_sha256':hashlib.sha256(BOOTSTRAP.encode()).hexdigest(),'argv_max_bytes':max(len(arg.encode()) for arg in command()),'source_fragments':len(command())-5,'cases':results}
    pathlib.Path(args.receipt).write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps({'passed':len(results),'receipt':args.receipt,'node':proof['node']}))
if __name__=='__main__':main()
