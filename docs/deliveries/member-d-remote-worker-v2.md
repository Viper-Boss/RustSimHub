# D25/26：实际远程 worker 与原 Core 授权接线

2026-11-02；用户批准 #56/#132 的离线实现。起点 #170 `3b1598f2e74fb63064b0f046b1111db2b4c06d1b`，本包在本人分支实现。为保持同一公共native入口只在一个PR，最终追加回同一个 #170，更新其最终范围，不另开重叠入口PR。完整受测源码、命令/计数、产物及失败证据在正式验证后绑定，不能用历史SDK结果冒充新HEAD。

[实现/协议/边界](../remote/worker-v2.md)。仅改明确分配的media授权/远程适配、native audio新独立模块/feature入口及本人夹具/文档；原LAN/v1、正常身份、电话/短信、Root服务与全局进度未改。Native依赖版本/lock不再改变，media仅增加原已有rustix的net feature。来源保留原AGPL及既有SDK上游许可证/作者；AI辅助实施/审查，不改Git作者。

目前实际开发验证已跑通：原Core Authority和协议/Client与真实TURN/DTLS/Opus；错误票据/boot/peer在PCM前拒绝；13类Core绑定撤权关闭PCM；控制EOF、重复id/跨session、超长/未知字段和迟到PCM拥有者保留。另有真实隔离UID984/985/986的三角色实际往返、原Authority撤权、非法UID984/986/987在空闲时拒绝；生产非测试CLI的真实Root拥有合成NSS、固定端点/0660、loopback配置拒绝、985正常解析、SIGTERM原inode清理也实际通过。这些初步命令在脏源码上运行，正式数字另绑。

夹具NSS只在临时镜像，Root控制器只持SETUID/SETGID/CHOWN/FOWNER（无CAP_KILL，清理helper先降到子进程同UID）；三个演员实际CapEff空。read-only/network=none、合成CA/凭据、PCM；未改宿主机NSS、未占生产账户、未安装设备。Core业务Context是合成、Authority与本地授权模块真实，不把它写成完整Core daemon/Owner/电话的验证。

标准test=false目标不会在普通cargo test启动Coturn。新的显式复跑入口 `native/audio-alsa/tests/run_private_remote_worker.py`，另以ignored角色函数在隔离容器按真实UID分别执行；ignore不是跳过后算通过。旧SDK23项仍单独报告，重复源码unit不累加成功能阶段。现有CI因Actions付款/额度未启动，#170 run41325796758及注释已回读；不改workflow或宣称绿。

当前阶段计数仍22/26离线已提交、剩D22/D23/D25/D26四项；固定P0–P7表80%。正式交付时单列可独立提交范围和外部前置，不用测试数量改分母。A生产安全provider/互操作、原DTLS依赖兼容修复/密码审查、正常远程信令与Root登记安装、真实PCM/设备/外网/两端听音保持未验收。
