# SPDX-License-Identifier: AGPL-3.0-only
"""Orchestrate the actual Rust test roles over private pipes, not a wire backend."""
import json
import os
import select
import subprocess
import time


def run(address, processes, as_uid):
    def launch(role,uid,**values):
        environment=dict(os.environ)
        environment.update(GATT_FIXTURE_ROLE=role,GATT_FIXTURE_BUS=address,**values)
        process=subprocess.Popen(['/input/gatt_kernel_uid','--exact','gatt_kernel_uid_role','--nocapture'],
            stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,bufsize=0,
            env=environment,preexec_fn=as_uid(uid))
        processes.append((process,uid));return process
    def wait(process,marker):
        deadline=time.monotonic()+7
        while time.monotonic()<deadline:
            ready,_,_=select.select([process.stdout],[],[],max(0,deadline-time.monotonic()))
            assert ready,'role output deadline: '+marker
            line=process.stdout.readline().decode('utf8','replace').strip()
            if not line:
                assert process.poll() is None,'role exited: '+process.stderr.read(8192).decode('utf8','replace')
                continue
            print(line,flush=True)
            if marker in line:return line.split(marker,1)[1].strip()
        raise AssertionError('missing role marker: '+marker)
    def command(process,value,marker):
        process.stdin.write((value+'\n').encode());process.stdin.flush()
        return wait(process,marker)
    def finish(process,value):
        process.stdin.write((value+'\n').encode());process.stdin.flush()
        out,err=process.communicate(timeout=4)
        text=out.decode('utf8','replace');print(text,end='',flush=True)
        assert process.returncode==0 and '1 passed; 0 failed' in text,err.decode('utf8','replace')
    bluez=launch('bluez',1003)
    owner=wait(bluez,'GATT_READY ');assert owner.startswith(':')
    ble=launch('ble',1002,GATT_FIXTURE_OWNER=owner)
    target=wait(ble,'GATT_READY ');assert target.startswith(':')
    foreign=launch('foreign',1004,GATT_FIXTURE_TARGET=target)
    out,err=foreign.communicate(timeout=4)
    text=out.decode('utf8','replace');print(text,end='',flush=True)
    assert foreign.returncode==0 and 'GATT_FOREIGN_DENIED' in text and '1 passed; 0 failed' in text,err.decode('utf8','replace')
    command(bluez,'ACQUIRE '+target,'GATT_ACQUIRED')
    command(ble,'NOTIFY','GATT_NOTIFY_ACCEPTED')
    command(bluez,'RECEIVE','GATT_PACKETS')
    command(bluez,'WRITE '+target,'GATT_WRITE_ACK')
    command(bluez,'DISCONNECT','GATT_DISCONNECTED')
    command(ble,'FENCE','GATT_FENCED')
    command(bluez,'CLOSED','GATT_FDS_CLOSED')
    finish(ble,'STOP');finish(bluez,'QUIT')
    print(json.dumps({'fixture':'rust-gatt-policy-real-uids','role_tests':3,
        'daemon_uid':1001,'ble_uid':1002,'bluez_uid':1003,'foreign_uid':1004,
        'policy_modified':False,'standard_acquire_signature':'a{sv} -> hq',
        'actual_rust_gatt':True,'role_effective_capabilities':0,'no_new_privileges':True}),flush=True)
