# SPDX-License-Identifier: AGPL-3.0-only
# Real systemd parser; temporary root and inert executables, no unit installation.
import pathlib,tempfile,subprocess,re,shlex,json
repo=pathlib.Path.cwd();overlay=repo/'firmware/system/overlay/etc/systemd/system';material=repo/'firmware/system/setup-ap'
with tempfile.TemporaryDirectory(prefix='rsh-ap-unit-verify-') as temp:
 root=pathlib.Path(temp);units=root/'etc/systemd/system';units.mkdir(parents=True)
 selected=['rustsimhub-core.service','rustsimhub-edge.service','rustsimhub-app.target']
 for name in selected:(units/name).write_bytes((overlay/name).read_bytes())
 for name in ['rustsimhub-setup-ap.service','rustsimhub-setup-edge.service']:(units/name).write_bytes((material/name).read_bytes())
 for service,source in [('rustsimhub-core.service','50-setup-ap-core.conf'),('rustsimhub-app.target','50-setup-ap-target.conf')]:
  directory=units/(service+'.d');directory.mkdir();(directory/'50-setup-ap.conf').write_bytes((material/source).read_bytes())
 for name in ['rustsimhub-app-select.service','ModemManager.service','NetworkManager.service','systemd-tmpfiles-setup.service','rustsimhub-app-recover.service','rustsimhub-media.service']:
  (units/name).write_text('[Unit]\nDefaultDependencies=no\n[Service]\nType=oneshot\nExecStart=/bin/true\n')
 for name in ['sysinit.target','basic.target','shutdown.target','network.target','network-online.target']:(units/name).write_text('[Unit]\nDefaultDependencies=no\n')
 for path in list(units.rglob('*.service'))+list(units.rglob('*.conf')):
  for line in path.read_text().splitlines():
   if line.startswith(('ExecStart=','ExecStartPre=')) and line.split('=',1)[1]:
    executable=shlex.split(line.split('=',1)[1])[0];target=root/executable.lstrip('/');target.parent.mkdir(parents=True,exist_ok=True)
    if not target.exists():target.write_text('#!/bin/sh\nexit 99\n');target.chmod(0o755)
 (root/'etc/passwd').write_text('root:x:0:0::/:/bin/sh\n'+''.join(f'rustsimhub-{name}:x:{uid}:{uid}::/:/bin/sh\n' for name,uid in [('core',986),('edge',985),('media',984),('forwarder',983)]))
 (root/'etc/group').write_text('root:x:0:\nrustsimhub-ipc:x:987:rustsimhub-core,rustsimhub-edge,rustsimhub-media\n')
 command=['systemd-analyze','--generators=no','--root='+str(root),'verify','rustsimhub-setup-ap.service','rustsimhub-setup-edge.service','rustsimhub-core.service','rustsimhub-app.target']
 result=subprocess.run(command,capture_output=True,timeout=20);out=result.stdout.decode('utf8','replace');err=result.stderr.decode('utf8','replace')
 report={'passed':result.returncode==0,'exit':result.returncode,'offline':True,'services_started':False,'executables_are_inert_fixtures':True,'stdout':out,'stderr':err}
 assert result.returncode==0,err
 print('Real systemd offline unit verification passed; no services or payloads executed.')
