# 持久SSH维护准备

板级首启只创建NSHSTATE私有`rustsimhub-v1/system/ssh-access`空目录。安装器必须在其中显式供给Root:Root/0600、单链接、单行有效Ed25519 `authorized_keys`；不继承旧密码、不将维护密钥或主机私钥放进通用树。当前尚未实现该安装器，也未把真实维护公钥安装到新系统。

`rustsimhub-ssh-prepare`先核对实际持久挂载、目录/公钥所有者与权限，再生成该设备的Ed25519主机密钥。密钥和公钥写入独立0700 pending目录，逐文件及目录同步后整体原子发布到`system/ssh-host`。合法完整pending可续用；既有主机密钥保持。任何不完整、冲突、错owner、软/硬链接、错公钥或额外文件均拒绝，保留现场，不自动重新生成或修复。

SSH启动要求prepare成功；缺维护公钥会使SSH保持未启动。配置只接受Root公钥登录，关闭密码/交互认证、代理和TCP转发、X11及隧道。该层不是远程更新或业务API，尚需真实受控维护登录、密钥更换/撤销与恢复验收。完整系统刷写前必须验证实际维护入口，不能依赖未配置的SSH。

新增4项隔离测试，使用真实生成的合成Ed25519密钥但不启动SSH；板级测试共30项。覆盖身份保持与完整pending续作、缺失/多行/选项/坏公钥拒绝、损坏及冲突主机材料保留、挂载守卫失败/错误owner/链接拒绝。

真实棒子的候选ARM用户态另做合成验证：相同helper只将持久路径改为一次性私有目录、挂载守卫显式替换为true（因此不是NSHSTATE或完整firstboot验收），重复运行host key摘要一致；实际候选ssh-keygen、sshd -t与-T通过，公钥认证及禁止转发等生效配置匹配。`.private/candidate-ssh-config-first.json`passed/cleanup_verified=true。没有启动SSH服务器、安装真实维护公钥或更改主机SSH；旧四服务active/PCMclosed，通话/短信/蜂窝零。
