#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Disposable private cgroup/netns test of ACTUAL Root provider and SDK.

Requires --privileged --cgroupns=private --network=none and read-only /inputs.
Globally-shaped IPs below exist only in this owned isolated network namespace.
No physical device, Internet, production identity, systemd or host route tested.
"""
import base64
import ctypes
import hashlib
import http.server
import json
import os
from pathlib import Path
import select
import shutil
import signal
import socket
import ssl
import struct
import subprocess
import sys
import threading
import time

CG=Path('/sys/fs/cgroup')
UNIT=CG/'system.slice/rustsimhub-nkn-root.service'
ROOT_SOCKET='/run/rustsimhub-nkn-root/root.sock'


def command(*args):
    return subprocess.check_output(args,stderr=subprocess.STDOUT,text=True)


def write(path,value):
    Path(path).write_text(str(value))


def exact(conn,n):
    result=bytearray()
    while len(result)<n:
        part=conn.recv(n-len(result))
        if not part:raise EOFError('owned Root connection closed')
        result.extend(part)
    return bytes(result)


def eventually(check,seconds=5):
    deadline=time.monotonic()+seconds
    while time.monotonic()<deadline:
        result=check()
        if result:return result
        time.sleep(.05)
    raise AssertionError('bounded fixture condition not observed')


def node():
    os.unshare(os.CLONE_NEWNET)
    write('/tmp/node-ready',os.getpid())
    eventually(lambda:Path('/tmp/node-configure').exists())
    command('/usr/sbin/ip','link','set','lo','up')
    for interface,ip in [('peer','9.9.9.9/24'),('otherpeer','9.9.8.9/24')]:
        command('/usr/sbin/ip','addr','add',ip,'dev',interface)
        command('/usr/sbin/ip','link','set',interface,'up')
    command('/usr/sbin/ip','addr','add','10.1.2.3/32','dev','peer')
    class Handler(http.server.BaseHTTPRequestHandler):
        protocol_version='HTTP/1.1'
        def log_message(self,*args):pass
        def do_POST(self):
            n=int(self.headers.get('Content-Length','0'));assert 0<n<4096
            request=json.loads(self.rfile.read(n));assert request['method']=='getwsaddr'
            body=json.dumps({'result':{'addr':'9.9.9.9:8000','pubkey':'11'*32,'id':'22'*32}}).encode()
            self.send_response(200);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
        def do_GET(self):
            key=self.headers['Sec-WebSocket-Key'];accept=base64.b64encode(hashlib.sha1((key+'258EAFA5-E914-47DA-95CA-C5AB0DC85B11').encode()).digest()).decode()
            self.send_response(101);self.send_header('Upgrade','websocket');self.send_header('Connection','Upgrade');self.send_header('Sec-WebSocket-Accept',accept);self.end_headers()
            def send(value):
                b=json.dumps(value).encode();header=bytes([0x81,len(b)]) if len(b)<126 else b'\x81\x7e'+struct.pack('>H',len(b));self.connection.sendall(header+b)
            send({'Action':'authChallenge','Error':0,'Challenge':'55'*32})
            first=exact(self.connection,2);size=first[1]&127
            if size==126:size=struct.unpack('>H',exact(self.connection,2))[0]
            assert size<4096 and first[1]&128
            mask=exact(self.connection,4);b=exact(self.connection,size);request=json.loads(bytes(v^mask[i%4] for i,v in enumerate(b)))
            assert request['Action']=='setClient' and len(request['Signature'])==128 and len(request['ClientSalt'])==64
            send({'Action':'setClient','Error':0,'Result':{'sigChainBlockHash':'33'*32}})
            try:
                while self.connection.recv(4096):pass
            except OSError:pass
    https=http.server.ThreadingHTTPServer(('9.9.9.9',8443),Handler);https.daemon_threads=True
    context=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);context.load_cert_chain('/inputs/tls.crt','/inputs/tls.key');https.socket=context.wrap_socket(https.socket,server_side=True)
    websocket=http.server.ThreadingHTTPServer(('9.9.9.9',8000),Handler);websocket.daemon_threads=True
    for server in [https,websocket]:threading.Thread(target=server.serve_forever,daemon=True).start()
    # Independently reachable targets distinguish a policy denial from a
    # missing fixture listener or nonexistent route.
    def tcp(ip):
        s=socket.socket();s.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1);s.bind((ip,9000));s.listen(8)
        while True:c,_=s.accept();c.close()
    def udp(port):
        s=socket.socket(socket.AF_INET,socket.SOCK_DGRAM);s.bind(('9.9.9.9',port))
        while True:b,a=s.recvfrom(64);s.sendto(b,a)
    for ip in ['9.9.9.9','9.9.8.9','10.1.2.3']:threading.Thread(target=tcp,args=(ip,),daemon=True).start()
    for port in [53,9001]:threading.Thread(target=udp,args=(port,),daemon=True).start()
    write('/tmp/node-serving','1')
    signal.pause()


def install_fixture():
    assert os.geteuid()==0 and Path('/.dockerenv').exists()
    assert Path('/proc/self/cgroup').read_text().strip()=='0::/'
    # Namespace-owned cgroup root is the Docker container scope. Never bind
    # the host cgroup filesystem into this test.
    (CG/'driver').mkdir();write(CG/'driver/cgroup.procs',os.getpid())
    write(CG/'cgroup.subtree_control','+cpu +memory +pids')
    (CG/'system.slice').mkdir();write(CG/'system.slice/cgroup.subtree_control','+cpu +memory +pids')
    UNIT.mkdir()
    for src in Path('/inputs/tools/lib').iterdir():
        dest=Path('/usr/lib/x86_64-linux-gnu')/src.name
        pending=dest.with_name(dest.name+'.owned-fixture-pending')
        shutil.copyfile(src,pending);pending.replace(dest)  # Preserve mapped old inodes.
    for src,dest in [('/inputs/tools/pkg/usr/sbin/nft','/usr/sbin/nft'),('/inputs/tools/ip','/usr/sbin/ip'),('/inputs/root','/root-provider'),('/inputs/core-test','/core-test')]:
        shutil.copyfile(src,dest);os.chmod(dest,0o755)
    for path in ['/opt/rustsimhub/selected','/etc/rustsimhub','/var/lib/rustsimhub/nkn','/run/rustsimhub-network','/run/rustsimhub-nkn-root']:
        Path(path).mkdir(parents=True,exist_ok=True);os.chmod(path,0o755)
    shutil.copyfile('/inputs/engine','/opt/rustsimhub/selected/nkn-sdk-engine');os.chmod('/opt/rustsimhub/selected/nkn-sdk-engine',0o555)
    shutil.copyfile('/inputs/tls.crt','/etc/ssl/certs/ca-certificates.crt')
    roles=[('rustsimhub-core',1001),('rustsimhub-edge',1003),('rustsimhub-forwarder',1004),('rustsimhub-ble',1006),('rustsimhub-media',1005),('rustsimhub-nkn',1002)]
    with open('/etc/passwd','a') as passwd,open('/etc/group','a') as groups:
        for name,uid in roles:passwd.write(f'{name}:x:{uid}:{uid}:fixture:/nonexistent:/usr/sbin/nologin\n');groups.write(f'{name}:x:{uid}:\n')
    os.chown('/run/rustsimhub-nkn-root',0,1001);os.chmod('/run/rustsimhub-nkn-root',0o750)
    seed=Path('/var/lib/rustsimhub/nkn/transport-seed');seed.write_bytes(bytes.fromhex('23'*32));seed.chmod(0o600)
    version=json.loads(command('/opt/rustsimhub/selected/nkn-sdk-engine','--version'))
    assert version['nkn_sdk_linked'] and not version['isolated_fixture_build']
    policy={'format':1,'source_commit':version['source_commit'],'engine_sha256':hashlib.sha256(Path('/inputs/engine').read_bytes()).hexdigest(),
            'core_pin':'12'*32,'statistics_public_key':'34'*32,'interface':'wlan0','seed_rpc':['https://9.9.9.9:8443']}
    raw=json.dumps(policy,separators=(',',':')).encode();Path('/etc/rustsimhub/nkn-policy.json').write_bytes(raw)
    policy_hash=hashlib.sha256(raw).digest()
    node_process=subprocess.Popen([sys.executable,__file__,'--node'],stdout=subprocess.DEVNULL,stderr=open('/tmp/node.log','wb'))
    eventually(lambda:Path('/tmp/node-ready').exists())
    for interface,peer,ip in [('wlan0','peer','9.9.9.8/24'),('usb0','otherpeer','9.9.8.8/24')]:
        command('/usr/sbin/ip','link','add',interface,'type','veth','peer','name',peer)
        command('/usr/sbin/ip','link','set',peer,'netns',str(node_process.pid))
        command('/usr/sbin/ip','addr','add',ip,'dev',interface);command('/usr/sbin/ip','link','set',interface,'up')
    command('/usr/sbin/ip','route','add','10.1.2.3/32','via','9.9.9.9','dev','wlan0')
    write('/tmp/node-configure','1');eventually(lambda:Path('/tmp/node-serving').exists())
    return policy_hash,node_process


class Core:
    def __init__(self,generation,policy_hash,nonce=None,deadline=None):
        self.g=generation;self.nonce=nonce or os.urandom(32);self.deadline=deadline or int(time.clock_gettime(time.CLOCK_BOOTTIME)*1000)+30000;self.policy=policy_hash
        read,writefd=os.pipe();reply_read,reply_write=os.pipe()
        pid=os.fork()
        if pid==0:
            try:
                os.close(writefd);os.close(reply_read);os.setgroups([]);os.setgid(1001);os.setuid(1001)
                c=socket.socket(socket.AF_UNIX);c.settimeout(4);c.connect(ROOT_SOCKET)
                source=os.fdopen(read);dest=os.fdopen(reply_write,'w',buffering=1)
                for line in source:
                    if line.strip()=='close':break
                    op=int(line);raw=b'RSCN\x01'+bytes([op])+struct.pack('>IQ',self.g,self.deadline)+self.nonce+self.policy
                    c.sendall(raw);b=exact(c,87)
                    actual_generation=struct.unpack('>I',b[5:9])[0]
                    assert b[:5]==b'RSHS\x01' and (self.g==0 or actual_generation==self.g) and b[17:49]==self.nonce and b[49:81]==self.policy
                    self.g=actual_generation
                    dest.write(json.dumps({'root':b[85],'engine':b[86],'generation':self.g,'pid':struct.unpack('>I',b[81:85])[0]})+'\n')
                c.close();os._exit(0)
            except BaseException:
                import traceback;traceback.print_exc();os._exit(1)
        os.close(read);os.close(reply_write);self.pid=pid;self.input=os.fdopen(writefd,'w',buffering=1);self.output=os.fdopen(reply_read)
    def request(self,op):
        self.input.write(str(op)+'\n');assert select.select([self.output],[],[],5)[0];line=self.output.readline();assert line,'Core did not receive original Root proof';return json.loads(line)
    def close(self):
        self.input.write('close\n');self.input.close();_,status=os.waitpid(self.pid,0);self.output.close();assert os.waitstatus_to_exitcode(status)==0


def empty():
    tables=json.loads(command('/usr/sbin/nft','-j','list','tables'))
    return not (UNIT/'engine').exists() and not any(row.get('table',{}).get('name')=='rustsimhub_nkn' for row in tables['nftables'])


def probe(uid):
    read,writefd=os.pipe();pid=os.fork()
    if pid==0:
        os.close(read);os.setgroups([]);os.setgid(uid);os.setuid(uid)
        results=[]
        for host,port,udp in [('9.9.9.9',9000,False),('10.1.2.3',9000,False),('9.9.8.9',9000,False),('9.9.9.9',53,True),('9.9.9.9',9001,True)]:
            s=socket.socket(socket.AF_INET,socket.SOCK_DGRAM if udp else socket.SOCK_STREAM);s.settimeout(.4)
            try:
                s.connect((host,port))
                if udp:s.send(b'owned');assert s.recv(64)==b'owned'
                results.append(True)
            except (OSError,AssertionError):results.append(False)
            finally:s.close()
        os.write(writefd,json.dumps(results).encode());os.close(writefd);os._exit(0)
    os.close(writefd);raw=os.read(read,4096);os.close(read);_,status=os.waitpid(pid,0);assert os.waitstatus_to_exitcode(status)==0;return json.loads(raw)


def main():
    if len(sys.argv)>1 and sys.argv[1]=='--node':node();return 0
    policy,node_process=install_fixture()
    def network_status():
        snapshot={'schema':1,'available':True,'boot_id':Path('/proc/sys/kernel/random/boot_id').read_text().strip(),'sample_unix_seconds':int(time.time()),
                  'devices':[{'interface':'wlan0','kind':'wifi','state':'100 (connected)','ipv4':['9.9.9.8/24'],'ipv6':[],'dns_ipv4':['9.9.9.9'],'dns_ipv6':[]}]}
        target=Path('/run/rustsimhub-network/status.json');pending=target.with_suffix('.pending');pending.write_text(json.dumps(snapshot));pending.replace(target)
    network_status()
    core_test=subprocess.Popen(['/core-test','--ignored','--exact','nkn_root::tests::root_ports_kernel_uid','--nocapture'],
        user=1001,group=1001,extra_groups=[],env=dict(os.environ,RSH_ROOT_PIN_REPORT='/tmp/owned-core-public-pin'),stdout=open('/tmp/core-test.log','wb'),stderr=subprocess.STDOUT)
    eventually(lambda:Path('/tmp/owned-core-public-pin').exists())
    document=json.loads(Path('/etc/rustsimhub/nkn-policy.json').read_bytes());document['core_pin']=Path('/tmp/owned-core-public-pin').read_text()
    raw=json.dumps(document,separators=(',',':')).encode();Path('/etc/rustsimhub/nkn-policy.json').write_bytes(raw);policy=hashlib.sha256(raw).digest()
    # Root helper starts in an actual namespace-owned cgroup. A real systemd
    # unit and physical NetworkManager producer are explicitly not claimed.
    root_pid=os.fork()
    if root_pid==0:
        write(UNIT/'cgroup.procs',os.getpid())
        assert ctypes.CDLL(None,use_errno=True).prctl(38,1,0,0,0)==0
        os.execv('/root-provider',['/root-provider','--serve-fixed'])
    eventually(lambda:Path(ROOT_SOCKET).exists())
    code=core_test.wait(timeout=15)
    if code:print(Path('/tmp/core-test.log').read_text(),flush=True)
    assert code==0,'actual Rust Root/supervisor integration failed'
    eventually(empty)
    results=[{'scenario':'actual-Rust-Auth-supervisor-three-references','actual_child_reaped':True,'released':True}]
    for generation,scenario in [(2,'explicit-stop'),(3,'Core-owner-EOF'),(4,'original-lease-expiry'),(5,'altered-policy-unknown')]:
        network_status();deadline=int(time.clock_gettime(time.CLOCK_BOOTTIME)*1000)+(4000 if scenario=='original-lease-expiry' else 30000)
        core=Core(0,policy,deadline=deadline)
        prepared=core.request(1);assert prepared['root']==1 and prepared['generation']==generation,prepared
        launched=core.request(2);assert launched['pid']>1 and launched['root']==1,launched
        sdk_pid=launched['pid']
        def ready():
            network_status();state=core.request(3)
            assert state['root']==1,state
            if state['engine']==3:
                print(json.dumps({'early_sdk_exit':state,'memory_events':(UNIT/'engine/memory.events').read_text(),'node_log':Path('/tmp/node.log').read_text()[:2000]}),flush=True)
                raise AssertionError('actual SDK exited before Root ready')
            return state if state['engine']==2 else None
        eventually(ready,8)
        status=Path(f'/proc/{sdk_pid}/status').read_text();assert '\nNoNewPrivs:\t1\n' in status and '\nCapEff:\t0000000000000000\n' in status
        assert Path(f'/proc/{sdk_pid}/cgroup').read_text().strip()=='0::/system.slice/rustsimhub-nkn-root.service/engine'
        assert (UNIT/'engine/memory.max').read_text().strip()=='134217728'
        if scenario=='explicit-stop':
            assert probe(0)==[True]*5,'denied fixture targets were not actually reachable'
            assert probe(1002)==[True,False,False,True,False],'kernel egress/interface/DNS fence failed'
            stopped=core.request(4);assert stopped['engine']==3,stopped
            released=core.request(5);assert released['root']==2 and released['engine']==3,released
            core.close();eventually(empty)
        elif scenario=='Core-owner-EOF':
            core.close();eventually(empty)
        elif scenario=='original-lease-expiry':
            eventually(empty,6);core.close()
        else:
            command('/usr/sbin/nft','add','rule','inet','rustsimhub_nkn','output','meta','skuid','1002','drop')
            def unknown():
                s=core.request(3);return s if s['root']==3 and s['engine']==3 else None
            eventually(unknown,6);assert not empty(),'altered policy was falsely released';core.close()
        eventually(lambda:not Path(f'/proc/{sdk_pid}').exists())
        results.append({'scenario':scenario,'actual_child_reaped':True,'released':scenario!='altered-policy-unknown'})
    os.kill(root_pid,signal.SIGTERM);os.waitpid(root_pid,0)
    node_process.terminate();node_process.wait(timeout=2)
    print(json.dumps({'actual_root_provider':True,'actual_sdk':True,'actual_kernel_cgroup_nft':True,
                      'public_network':False,'physical_device':False,'real_systemd_unit':False,'results':results}),flush=True)
    return 0


if __name__=='__main__':
    sys.exit(main())
