# SPDX-License-Identifier: AGPL-3.0-only
"""Manual isolated-container policy experiment, not a GATT implementation.

libdbus marshals real messages and SCM_RIGHTS. Synthetic method bodies isolate
header-policy behavior; this does not replace production Rust/BlueZ tests.
"""
import ctypes as C
import json
import os
from pathlib import Path
import select
import socket
import subprocess
import sys
import tempfile
import time
import xml.etree.ElementTree as ET

ROOT = '/org/rustsimhub/recovery/v1'
CHAR = ROOT+'/service0/char1'
BROKER = b'org.freedesktop.DBus'
LIB = None


def bind():
    global LIB
    LIB = C.CDLL('/input/lib/libdbus-1.so.3')
    # Only public stable libdbus C ABI, kept entirely in this test process.
    definitions = {
        'dbus_connection_open_private': (C.c_void_p, [C.c_char_p, C.c_void_p]),
        'dbus_bus_register': (C.c_int, [C.c_void_p, C.c_void_p]),
        'dbus_bus_get_unique_name': (C.c_char_p, [C.c_void_p]),
        'dbus_bus_request_name': (C.c_int, [C.c_void_p, C.c_char_p, C.c_uint, C.c_void_p]),
        'dbus_bus_add_match': (None, [C.c_void_p, C.c_char_p, C.c_void_p]),
        'dbus_connection_set_exit_on_disconnect': (None, [C.c_void_p, C.c_int]),
        'dbus_connection_send': (C.c_int, [C.c_void_p, C.c_void_p, C.c_void_p]),
        'dbus_connection_flush': (None, [C.c_void_p]),
        'dbus_connection_read_write': (C.c_int, [C.c_void_p, C.c_int]),
        'dbus_connection_pop_message': (C.c_void_p, [C.c_void_p]),
        'dbus_connection_close': (None, [C.c_void_p]),
        'dbus_connection_unref': (None, [C.c_void_p]),
        'dbus_message_new_method_call': (C.c_void_p, [C.c_char_p]*4),
        'dbus_message_new_method_return': (C.c_void_p, [C.c_void_p]),
        'dbus_message_new_signal': (C.c_void_p, [C.c_char_p]*3),
        'dbus_message_get_type': (C.c_int, [C.c_void_p]),
        'dbus_message_get_signature': (C.c_char_p, [C.c_void_p]),
        'dbus_message_get_member': (C.c_char_p, [C.c_void_p]),
        'dbus_message_get_error_name': (C.c_char_p, [C.c_void_p]),
        'dbus_message_unref': (None, [C.c_void_p]),
        # These two APIs are variadic; declare their fixed prefix only.
        'dbus_message_append_args': (C.c_int, [C.c_void_p, C.c_int]),
        'dbus_message_get_args': (C.c_int, [C.c_void_p, C.c_void_p, C.c_int]),
    }
    for name, (result, args) in definitions.items():
        fn = getattr(LIB, name); fn.restype = result; fn.argtypes = args


class Bus:
    def __init__(self, address):
        self.conn = LIB.dbus_connection_open_private(address.encode(), None)
        assert self.conn, 'open actual private bus'
        LIB.dbus_connection_set_exit_on_disconnect(self.conn, 0)
        class Error(C.Structure):
            _fields_=[('name',C.c_char_p),('message',C.c_char_p),('bits',C.c_uint),('padding',C.c_void_p)]
        error=Error()
        LIB.dbus_error_init.argtypes=[C.POINTER(Error)];LIB.dbus_error_init(C.byref(error))
        success=LIB.dbus_bus_register(self.conn,C.byref(error))
        detail=(error.name,error.message)
        LIB.dbus_error_free.argtypes=[C.POINTER(Error)];LIB.dbus_error_free(C.byref(error))
        assert success, ('actual Hello',detail)
        self.unique = LIB.dbus_bus_get_unique_name(self.conn).decode()

    def close(self):
        LIB.dbus_connection_close(self.conn); LIB.dbus_connection_unref(self.conn)

    def call(self, dest, path, interface, member, argument=None, incoming_fd=None):
        msg = LIB.dbus_message_new_method_call(*(v.encode() for v in [dest, path, interface, member]))
        assert msg
        try:
            if argument is not None:
                value = C.c_char_p(argument.encode())
                assert LIB.dbus_message_append_args(msg, ord('s'), C.byref(value), C.c_int(0))
            if incoming_fd is not None:
                fd=C.c_int(incoming_fd)
                assert LIB.dbus_message_append_args(msg,ord('h'),C.byref(fd),C.c_int(0))
            # Use an explicit pending call so broker error names survive.
            return self.pending(msg)
        finally:
            LIB.dbus_message_unref(msg)

    def pending(self, msg):
        # Bounded actual broker reply, including AccessDenied and NoReply.
        fn = LIB.dbus_connection_send_with_reply
        fn.restype = C.c_int
        fn.argtypes = [C.c_void_p, C.c_void_p, C.POINTER(C.c_void_p), C.c_int]
        pending = C.c_void_p()
        assert fn(self.conn, msg, C.byref(pending), 700) and pending
        block = LIB.dbus_pending_call_block; block.argtypes = [C.c_void_p]; block.restype = None
        steal = LIB.dbus_pending_call_steal_reply; steal.argtypes = [C.c_void_p]; steal.restype = C.c_void_p
        unref = LIB.dbus_pending_call_unref; unref.argtypes = [C.c_void_p]; unref.restype = None
        block(pending); reply = steal(pending); unref(pending)
        assert reply, 'bounded pending reply'
        return reply

    def send(self, msg):
        try:
            assert LIB.dbus_connection_send(self.conn, msg, None)
            LIB.dbus_connection_flush(self.conn)
        finally:
            LIB.dbus_message_unref(msg)

    def messages(self, milliseconds):
        deadline = time.monotonic()+milliseconds/1000
        while time.monotonic() < deadline:
            assert LIB.dbus_connection_read_write(self.conn, 10), 'private bus alive'
            msg = LIB.dbus_connection_pop_message(self.conn)
            if msg:
                try: yield msg
                finally: LIB.dbus_message_unref(msg)


def status(msg):
    error = LIB.dbus_message_get_error_name(msg)
    return error.decode() if error else LIB.dbus_message_get_signature(msg).decode()


def role():
    mode, address = sys.argv[2:4]
    assert os.getuid() in (1002,1003,1004)
    proc = Path('/proc/self/status').read_text()
    assert 'CapEff:\t0000000000000000' in proc and 'NoNewPrivs:\t1' in proc
    bind(); bus = Bus(address)
    try:
        if mode == 'ble':
            print(bus.unique, flush=True)
            writers = []
            for msg in bus.messages(20000):
                if LIB.dbus_message_get_type(msg) != 1: continue
                text = C.c_char_p()
                assert LIB.dbus_message_get_args(msg, None, ord('s'), C.byref(text), C.c_int(0))
                action = text.value.decode()
                reply = LIB.dbus_message_new_method_return(msg)
                if action in ('fd_one', 'fd_two'):
                    for _ in range(1 if action == 'fd_one' else 2):
                        local, peer = socket.socketpair(socket.AF_UNIX, socket.SOCK_SEQPACKET)
                        fd = C.c_int(peer.fileno())
                        assert LIB.dbus_message_append_args(reply, ord('h'), C.byref(fd), C.c_int(0))
                        peer.close(); local.send(b'POLICY_UID_FD'); writers.append(local)
                elif action in ('signal_value', 'signal_metadata'):
                    if action == 'signal_value':
                        signal = LIB.dbus_message_new_signal(CHAR.encode(), b'org.freedesktop.DBus.Properties', b'PropertiesChanged')
                    else:
                        signal = LIB.dbus_message_new_signal(ROOT.encode(), b'org.freedesktop.DBus.ObjectManager', b'InterfacesAdded')
                    bus.send(signal)  # no-body synthetic probe; policy only.
                else:
                    assert action == 'empty'
                bus.send(reply)
            for writer in writers: writer.close()
        else:
            dest = sys.argv[4]
            if mode == 'bluez':
                assert LIB.dbus_bus_request_name(bus.conn, b'org.bluez', 0, None) == 1
                LIB.dbus_bus_add_match(bus.conn, b"type='signal'", None)
                for msg in bus.messages(40): pass  # drain initial NameAcquired
                def call(path=CHAR, member='AcquireNotify', action='empty'):
                    return bus.call(dest,path,'org.bluez.GattCharacteristic1',member,action)
                one = call(action='fd_one')
                try:
                    assert status(one) == 'h', status(one)
                    fd = C.c_int(-1)
                    assert LIB.dbus_message_get_args(one,None,ord('h'),C.byref(fd),C.c_int(0))
                    with socket.socket(fileno=fd.value) as peer:
                        peer.settimeout(1); assert peer.recv(64) == b'POLICY_UID_FD'
                finally: LIB.dbus_message_unref(one)
                print('POLICY_CASE one_fd_unique_owner_allowed', flush=True)
                two = call(action='fd_two')
                try:
                    assert status(two) == 'org.freedesktop.DBus.Error.NoReply', status(two)
                finally: LIB.dbus_message_unref(two)
                print('POLICY_CASE two_fds_blocked', flush=True)
                for path, member, label in [(ROOT+'/unrelated','AcquireNotify','wrong_path'),
                                            (ROOT+'/service0/char0','AcquireNotify','wrong_characteristic'),
                                            (CHAR,'AcquireWrite','wrong_member')]:
                    reply = call(path,member)
                    try: assert status(reply) == 'org.freedesktop.DBus.Error.AccessDenied', status(reply)
                    finally: LIB.dbus_message_unref(reply)
                    print('POLICY_CASE '+label+'_denied', flush=True)
                with socket.socket(socket.AF_UNIX,socket.SOCK_DGRAM) as peer:
                    reply=bus.call(dest,CHAR,'org.bluez.GattCharacteristic1','AcquireNotify','empty',peer.fileno())
                    try: assert status(reply) == 'org.freedesktop.DBus.Error.AccessDenied',status(reply)
                    finally: LIB.dbus_message_unref(reply)
                print('POLICY_CASE incoming_fd_denied',flush=True)
                for action, expected in [('signal_value',False),('signal_metadata',True)]:
                    reply = call(action=action)
                    try: assert status(reply) == '', status(reply)
                    finally: LIB.dbus_message_unref(reply)
                    members = [LIB.dbus_message_get_member(msg) for msg in bus.messages(100)
                               if LIB.dbus_message_get_type(msg) == 4]
                    # A blocked signal can generate a broker error for its
                    # sender, but never arrives at this subscribed BlueZ UID.
                    self_member = b'PropertiesChanged' if action == 'signal_value' else b'InterfacesAdded'
                    assert (self_member in members) is expected, (action,members)
                    print('POLICY_CASE '+action+('_allowed' if expected else '_denied'), flush=True)
            else:
                reply=bus.call(dest,CHAR,'org.bluez.GattCharacteristic1','AcquireNotify','empty')
                try: assert status(reply) == 'org.freedesktop.DBus.Error.AccessDenied', status(reply)
                finally: LIB.dbus_message_unref(reply)
                print('POLICY_CASE foreign_uid_denied', flush=True)
    finally:
        bus.close()


def main():
    if len(sys.argv)>1 and sys.argv[1] == 'role': return role()
    assert os.getuid() == 0 and Path('/.dockerenv').exists(), 'container setup only'
    processes = []
    with tempfile.TemporaryDirectory(prefix='policy-uid-',dir='/tmp') as temp:
        base=Path(temp); base.chmod(0o755)
        config=ET.Element('busconfig')
        ET.SubElement(config,'type').text='system'
        ET.SubElement(config,'listen').text='unix:path='+str(base/'bus')
        ET.SubElement(config,'auth').text='EXTERNAL'
        default=ET.SubElement(config,'policy',{'context':'default'})
        ET.SubElement(default,'allow',{'user':'*'})
        # Closed default except fixture BlueZ/attacker clients. The production
        # BLE user policy is inserted unchanged. The read-only synthetic NSS
        # maps its real role name to fixture UID1002, not production UID982.
        for uid in ('fixture-bluez','fixture-foreign'):
            p=ET.SubElement(config,'policy',{'user':uid})
            for rule in [{'send_destination':'*'},{'receive_sender':'*'}]: ET.SubElement(p,'allow',rule)
        ET.SubElement(config[-2],'allow',{'own':'org.bluez'})
        policy=ET.fromstring(Path('/input/policy.in').read_bytes())[0]
        assert policy.attrib == {'user':'rustsimhub-ble'}
        config.append(policy)
        cfg=base/'bus.conf';cfg.write_bytes(ET.tostring(config));cfg.chmod(0o644)
        def as_uid(uid):
            def drop():
                os.setgroups([]); os.setgid(uid); os.setuid(uid)
            return drop
        errors=base/'daemon.log'
        with errors.open('w+') as log:
            os.chown(base,1001,1001)
            daemon=subprocess.Popen(['/input/lib/ld-linux-x86-64.so.2','--library-path','/input/lib',
                '/input/dbus-daemon','--config-file='+str(cfg),'--nofork','--print-address=1','--nopidfile'],
                stdout=subprocess.PIPE,stderr=log,text=True,preexec_fn=as_uid(1001))
            processes.append((daemon,1001))
            try:
                daemon_status=Path(f'/proc/{daemon.pid}/status').read_text()
                assert 'CapEff:\t0000000000000000' in daemon_status
                assert 'NoNewPrivs:\t1' in daemon_status
                assert next(line for line in daemon_status.splitlines() if line.startswith('Uid:')).split()[1:] == ['1001']*4
                ready,_,_=select.select([daemon.stdout],[],[],3); assert ready,'daemon startup bound'
                address=daemon.stdout.readline().strip(); assert address.startswith('unix:path='),errors.read_text()
                def launch(mode,uid,*args):
                    child=subprocess.Popen([sys.executable,'/input/fixture.py','role',mode,address,*args],
                        stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,preexec_fn=as_uid(uid))
                    processes.append((child,uid));return child
                ble=launch('ble',1002)
                ready,_,_=select.select([ble.stdout],[],[],3);assert ready,'BLE Hello deadline'
                unique=ble.stdout.readline().strip();assert unique.startswith(':'),ble.stderr.read(4096)
                for mode,uid in [('bluez',1003),('foreign',1004)]:
                    child=launch(mode,uid,unique)
                    out,err=child.communicate(timeout=8)
                    print(out,end='',flush=True);assert child.returncode==0,err
                print(json.dumps({'fixture':'ble-policy-real-uids','cases':9,
                    'daemon_uid':1001,'ble_uid':1002,'bluez_uid':1003,'foreign_uid':1004,
                    'role_effective_capabilities':0,'no_new_privileges':True}),flush=True)
            finally:
                def stop(process,uid,number):
                    subprocess.run([sys.executable,'-c',
                        'import os,sys; os.kill(int(sys.argv[1]),int(sys.argv[2]))',
                        str(process.pid),str(number)],preexec_fn=as_uid(uid),
                        stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=2,check=False)
                for process,uid in reversed(processes):
                    if process.poll() is None: stop(process,uid,15)
                    try: process.wait(timeout=2)
                    except subprocess.TimeoutExpired: stop(process,uid,9);process.wait(timeout=2)
                    for stream in [process.stdout,process.stderr]:
                        if stream:stream.close()
                # Fixed fixture daemon diagnostics only, bounded/no production
                # data. This also makes policy delivery failures reviewable.
                log.flush();log.seek(0);print('PRIVATE_BUS_LOG\n'+log.read(8192),flush=True)
                os.chown(base,0,0)  # restore only this temporary fixture for cleanup


if __name__=='__main__': main()
