# P4W 私有目录下的有限离线准备任务

2026-10-26，八文件热点安装第一次实际执行在离线 Core 准备处被拒绝：版本仓库和持久状态的父目录刻意为 Root `0700`，直接 `runuser` 无法遍历。先前 chroot 的父目录过于宽松，未暴露这一差异。实际安装未发布激活配置；固定回退删除本轮已写目标，保留业务库，应用50恢复运行，600秒恢复守卫已解除。未进行网络切换。

## 修正边界

- 只修改固定安装脚本及交付说明，不改变父目录权限，不让 Core/Edge 准备程序以 Root 运行。
- PID1创建两个有限、自动回收的准备任务：各角色只能挂载自己的目录和已验签选中程序，UID/GID分别为既有Core/Edge身份；空capabilities、NoNewPrivileges、严格只读系统、私有设备/临时目录/网络、15秒运行上限和3秒停止上限。
- 使用固定 `/usr/bin/env -i` 入口在命名空间建立后执行固定程序，解决 `systemd-run` 客户端在挂载前查找别名程序的问题。没有任意命令或路径API。
- `preparation_jobs_started: true` 明示有限准备任务；`services_started: false` 表示尚未启动持久业务/热点服务。两个任务结束、材料完整和原身份核对后，才发布最后两份激活配置。
- 八份已签名程序/配置仍绑定实际应用50的0edfc54/144e451归档；本轮维护脚本另固定当前提交与摘要，不将旧二进制改标为新构建。

## 已执行

10项准入/签名交付通过。10项真实Core/Edge/AP离线CLI安装、断点及精确回退通过；夹具现在把版本/状态父目录设为Root0700，使用实际私有mount命名空间和真实UID，PID1仍为显式替身，不能冒充真实systemd限制测试。夹具最初的chroot根传播及077 umask别名父目录失败保留。

另外在棒子上执行四个真实、有限PID1任务：同源Core构建信息、通过角色挂载读取自己的身份文件、Edge实际UID985及自己的目录写权限均通过；不读取身份内容、不写业务文件、不切网，原0700父目录与正常电话/短信服务保持。第一次挂载前程序路径查找失败同样保留。

先前P4V的CI40325758049在既有启动器16KiB边界测试失败；后续T3完整提交[CI40327585407](https://github.com/Viper-Boss/RustSimHub/actions/runs/40327585407)两任务成功。该边界偶发原因尚未证实，未扩大输出上限或截止，不能将重跑成功记为修复。

## 最终实机交付（2026-10-27）

固定脚本提交 `b5cb0a502f8e9d000e82d862587f63f7bd909d8e` 的[完整CI40330027498](https://github.com/Viper-Boss/RustSimHub/actions/runs/40330027498)两任务成功。回退遗留的六个目录逐个核对为空且拥有者/模式精确匹配后，仅用rmdir移除；没有删除文件或业务数据。随后重新备份、回读、准入、独立验签并上传28文件交付，实际只读预检通过。

第二次实际安装通过：八个固定目标回读一致，Core为非Root联合模式，普通Edge/媒体/配网Edge均无有效capabilities，Root热点执行程序只具CAP_NET_ADMIN，五个服务均NoNewPrivileges。Core/Edge离线准备由上述有限PID1任务完成；原身份、所有者、setup.used和普通TLS保持，恢复守卫已解除，原厂16、confirmed50和恢复预算保持。

完整健康检查通过：14个普通HTTPS资源精确匹配真实生产源码；配网Edge在设备回环口的HTTPS经过现有OpenSSL证书与IP校验，页面精确匹配Git，客户端退出0。设备没有curl且维护SSH禁用TCP转发，保留这些设置；用设备本地既有OpenSSL验证，没有安装工具、改SSH或系统证书库。最初探针缺curl、转发拒绝和文本/字节流夹具错误均保留，不作为设备故障或成功证据。

主库完整性、一个原Owner、69条短信、七个保护表（含Unknown发送/通话回执）、原Bark策略和唯一confirmed回执均与新备份一致；转发OFF且无worker/临时凭据，普通数据断开，音频7/7。网络连接配置与活动连接集合未变，没有启动AP、试连STA、保存Wi-Fi、拨号、发送短信或推送，也未重启/刷分区。浏览器插件本轮接管超时，没有替代成用户已听音/点击验收。

AP-06的“同源/签名/权限核验及联合安装材料”验收已满足。其完成不包含AP-03的保存与冷启动，也不包含AP-07/08的真实热点隔离、手机接入、地址/证书/身份交接。原P0–P7仍80%；扩展动作完成10/169，剩159；实际工时未独立计时，保持未知。

私有索引：`receipt-delivery-maintenance-health-confirm-20261026-first.json`、`setup-ap-installation-maintenance-backup-20261026-second.json`、`setup-ap-installation-assembly-20261026-third.json`、`setup-ap-installation-upload-check-20261026-second.json`、`setup-ap-installation-apply-20261026-second.json`、`setup-ap-installation-health-20261027-fifth.json`和`ap-openssl-probe-20261026-first.json`；原始日志、身份和备份不公开。
