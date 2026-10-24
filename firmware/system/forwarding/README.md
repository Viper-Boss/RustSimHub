# 转发服务安装候选（尚未安装）

这些文件特意不在自动复制的`overlay/`中，未安装、未启用，也不属于当前已签名应用组。源码已通过本机真实systemd/凭据/cgroup验证，范围及原失败见[验证记录](../../../docs/VALIDATION-FORWARDING-SUPERVISOR-X1F.md)，不能据此跳过安装和设备验收。

- worker由PID1以专用用户`rustsimhub-forwarder`启动，UID/GID预定983，附加IPC组987。其CapabilityBoundingSet和AmbientCapabilities均为空，NoNewPrivileges开启；Root不驻留，不承担身份切换。
- Root reconcile/status/boot是有限oneshot，能力集合均为空，只控制固定worker单元。Core最终只可启动reconcile/status两个管理单元；待接真实Core管理与网页后才能安装相应polkit规则。
- Root将有限配置写入`/run/rustsimhub-forwarding/settings.json`（0600），PID1通过LoadCredential提供每个服务私有的只读副本。argv和环境变量不包含令牌；环境只含凭据目录路径。关闭后停整个cgroup、核对专用UID进程并删除运行时配置。
- 根目录的持久`enabled.json`默认不存在。app.target候选drop-in仅请求boot oneshot；条件不满足时不执行进程，不创建转发timer/socket/path。

安装前必须核验：服务用户/组无数字ID冲突；当前Core显式启用受限worker UID通道；Root私有持久目录、runtime目录、只读CA及固定接口选择均正确；systemd至少支持LoadCredential（247+）；签名应用组包含同源forwarder与兼容Core；所有启停/故障/完整关闭及设备冷启动验收已完成。缺任一项时拒绝安装，不能用测试夹具或旧Root启动器替代。
