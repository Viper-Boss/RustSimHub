# 新系统签名业务交付与设备受限运行 P6W

2026-10-22，用户追加“再做一个”。本轮通过严格固定公钥的现有USB维护SSH，在已经安装的新OS内交付经过P6U/P6V验证的签名业务组及必要维护文件，并在棒子上运行受限检查。没有重新刷写p14/p15，没有安装整张P6V镜像，没有初始化所有者或启动正式业务。

## 已交付范围

| 对象 | 实际结果 |
|---|---|
| factory业务组 | `/usr/share/rustsimhub/factory-app`从原4e6/epoch1更新为2b/epoch7，四程序及Opus许可、manifest和签名合计7文件 |
| 同源业务来源 | `2b3c49cc469bce5e0f59ae377f9a59db9f9d0a89`；归档 `5d61a14119580fe216e5922961cf537a0b0673ec9df33e610ed731f23dfb0f61` |
| 已安装manifest | `cee2f68e30eafbe61f0871982398c14060ffee52cf6c9fbb8920fef877eba256`，epoch7，schema5，fixtures=false |
| 六程序工具slot | `/opt/rustsimhub/factory/5d61a14119580fe216e5922961cf537a0b0673ec9df33e610ed731f23dfb0f61`，Core/ctl/Edge/update/recovery/migrate及build.json；旧slot保留 |
| 6个配套文件 | 不可变update、edge-setup、migrate，Core单元、tmpfiles配置及Root灯控 |
| Root灯控来源 | `0c817c1af2d68380cfcdb7de54e0f14d67372baa`，归档 `366e73414246c5c28fe73330dfc4b7d4af8f28e0bfe1037c4a1f3d416787190e` |
| 灯控二进制SHA256 | `92166a82a699120efa25f4bb16f88d48191370950f097f6edb5f8de74ce8cfe4` |

签名公钥保持原来已选开发身份，先验旧包，再用新更新器验暂存新包，交付后再次验签和核对每个文件。没有更换公钥、上传签名私钥或创建新的设备/用户身份。epoch7目前是factory manifest版本，**不表示应用台账已创建或floor已推进**。

## 交付方式与恢复材料

只读预检确认维护公钥匹配、业务未初始化、Core/Edge/Media及选择器inactive、调制解调器注册home、无活动通话、PCM全closed、普通gsm未连接，Root文件系统有足够空间。

旧factory的7文件和6个配套文件共 **13份实际设备文件**，在任何目标替换前复制到PC私有备份，核对来源摘要、Root所有者、权限、单链接及备份回读。关键未改配套脚本/单元也与候选摘要核对。

新文件使用不存在的路径独占暂存，逐项核摘要、设置权限并同步。factory通过GNU mv的 `--exchange --no-copy -T` 执行一次原子目录交换，旧目录随交换保留；工具slot新建，配套文件逐个同目录原子替换，均同步文件及父目录。未初始化门禁在交付期间保持；如中断或结果未知，必须检查唯一报告，不重新执行helper。

之后仅reload单元、按固定tmpfiles配置创建提示目录并重启灯控；没有重启NetworkManager、ModemManager、USB或SSH，没有启动Core/Edge/Media业务组。没有原始块设备写入、GPT/boot/基带/NV/校准区操作或蜂窝APN激活。

恢复材料：

- PC备份：`.private/r01c-backup-20261022-first/`，每文件原摘要/权限/对应位置在交付索引中。
- 设备旧factory：`/usr/share/rustsimhub/.factory-app-r01c-2b3c49c-first`，交换后核旧文件字节保持并将目录设为Root0700。
- Root私有交付收据：`/usr/share/rustsimhub/maintenance-receipts/r01c-20261022.json`，Root0600单链接，原子发布后完整回读；PC副本为 `.private/r01c-device-receipt-20261022-first.json`。

不要自动删除旧材料；一旦后续完成初始化、台账推进或产生数据，就不能仅交换回旧factory来宣称恢复完成。恢复必须同时考虑状态、版本floor和身份兼容。

## 设备上完成的验证

| 验证 | 实际结果 |
|---|---|
| 新旧签名与文件 | 旧包、暂存新包和已交付新包验签通过；公钥、13份旧备份、新文件及slot摘要/权限核对通过 |
| 六个程序来源 | 以Core UID986执行build-info，均为2b归档；生产Core fixtures=false |
| Core自检 | UID986执行，ok=true、hardware_mutations=false |
| Native隔离 | 实际棒子上UID984临时单元，空capabilities、PrivateDevices、PrivateNetwork、私有tmp、只读系统、屏蔽D-Bus与真实业务数据/IPC |
| Opus合成数据 | 500帧，解码峰值大于0，PCM未打开 |
| DTLS/SRTP合成数据 | 私有网络loopback，双向各收到50帧；指纹、错误授权拒绝、撤销关闭及媒体超时关闭通过；无真实电话声音 |
| 临时测试单元 | 有40秒上限，运行退出0后collect；最终LoadState=not-found |
| Root灯控 | 新0c来源匹配，enabled/active/running、空capabilities，8秒GPIO电平采样包含111/000；不是本轮新的肉眼确认 |
| 提示目录 | `/run/rustsimhub-led-input`为986:986:0700，更新提示目录为0:0:0700；真实业务联动尚未启用 |
| 初次设置门禁 | 在installation.complete不存在时实际请求选择器启动，ConditionResult=no、inactive，未创建state.json、未执行业务启动 |
| 收尾状态 | boot_id不变，仍home注册、无通话、PCMclosed、普通gsm未连接、Core/Edge/Media仍inactive，identity/state/setup目录保持空 |

这是设备上的新二进制/隔离合成检查，不是完整应用启动、真实上下行声音、短信、冷启动或长期稳定性验收。累计10010仍38，本轮新通话/短信均0。

## 唯一接续记录

- 交付：`.private/r01c-delivery-20261022-first.json` passed=true。
- 初次设置门禁及收据：`.private/r01c-final-gate-20261022-first.json` passed=true。
- 只读完整基线：`.private/r01c-readonly-20261022-third.json`；前两次findmnt参数/固定modem0诊断失败保留，最终按实际枚举对象查询，没有为诊断而重启modem。
- 目录交换能力：`.private/r01c-capabilities-20261022-second.json`。首轮能力探针因复用不适用的业务状态断言失败，未修改设备；失败记录保留。
- 实际安装第三索引与 `.private/third-attempt-continuation-20261021-first.json` 追加交付overlay和新灯控来源，原安装出处不改写。

设备上原 `system-candidate.json`仍记录bbb安装组成，不能据此认定当前factory仍4e6；以本次交付收据和私有索引判断维护后状态。当前应描述为 **bbb安装系统加既有维护补丁及本轮签名业务/配套文件交付**，不得声称物理rootfs等于P6V镜像或其树摘要。

## 下一项与进度

下一项是首次所有者/监听端点/TLS与媒体配置，沿用维护与签名身份，使用本轮已交付的初始化工具和factory。先确定USB/Wi-Fi访问及证书交付方式，再完成初始化、业务启动、授权健康检查和冷启动；不得复用旧preview设置码、旧owner会话或已消费初始化/安装器，不能在发现残留身份时重置它。

本轮未向客户端开放新服务、未导入浏览器证书、未创建所有者，也没有请求用户拔插。R01剩余业务初始化及实机端到端验收；完整系统更新/恢复门禁不因本次文件交付而改为通过。

P6从7.5/12计至8/12，其他权重保持，整体 **75.5/100，约75.5%**。增加的是实际签名交付及设备受限检查，真实业务与冷启动点数保留；保存此单项后不自动继续下一项。
