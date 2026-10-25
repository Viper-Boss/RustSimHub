# 转发服务与有限首次安装

这些文件特意不在自动复制的`overlay/`中，未安装、未启用，也不属于当前已签名应用组。源码已通过本机真实systemd/凭据/cgroup验证，范围及原失败见[验证记录](../../../docs/VALIDATION-FORWARDING-SUPERVISOR-X1F.md)，不能据此跳过安装和设备验收。

- worker由PID1以专用用户`rustsimhub-forwarder`启动，UID/GID预定983，附加IPC组987。其CapabilityBoundingSet和AmbientCapabilities均为空，NoNewPrivileges开启；Root不驻留，不承担身份切换。
- Root reconcile/status/boot是有限oneshot，能力集合均为空，只控制固定worker单元。Core最终只可启动reconcile/status两个管理单元；[Core显式管理模式](../../../docs/VALIDATION-FORWARDING-COORDINATION-X1G.md)已通过真实本机链路；所有者网页和一键合成测试已有源码/本机链路证据；签名安装和实机仍缺，尚未安装生产polkit规则。
- Root将有限配置写入`/run/rustsimhub-forwarding/settings.json`（0600），PID1通过LoadCredential提供每个服务私有的只读副本。argv和环境变量不包含令牌；环境只含凭据目录路径。关闭后停整个cgroup、核对专用UID进程并删除运行时配置。
- 根目录的持久`enabled.json`默认不存在。app.target候选drop-in仅请求boot oneshot；条件不满足时不执行进程，不创建转发timer/socket/path。

安装前必须核验服务用户/组无数字ID冲突、备份可回读、签名应用已确认且兼容管理参数、目录与只读CA及固定接口选择正确、systemd支持LoadCredential（247+）。安装后仍需真实默认OFF、启停/故障/完整关闭及设备冷启动验收；不能用测试夹具或旧Root启动器代替。

2026-10-25：[X1L](../../../docs/VALIDATION-FORWARDING-DEVICE-X1L.md)已将冻结六文件应用组部署并确认epoch47，原Owner及短信保持；不代表Root服务安装。[X1M安装器](../../../docs/VALIDATION-FORWARDING-INSTALLER-X1M.md)新增固定材料生成器和`install.sh`，14项本机测试通过，包含实际Windows清单字节测试；实际安装结果按后续设备记录判定。

维护电脑使用`tools/system/forwarding_install.py --inventory 私有清单 --backup 已验证归档 --output 新的绝对目录`生成包。复制到棒子Root独占0700目录，各文件0600后，使用`sh 包目录/apply.sh 包目录 --check`只读检查。停止应用组并设好有限恢复动作后才执行`--apply`。首次安装拒绝所有已有目的地，不能拿它覆盖升级或自动修复半安装账户。

脚本不启动服务，不安装候选`sysusers.conf`来隐式分配其他UID，不创建enabled标记。`--rollback`仅撤销摘要完全匹配的九个固定文件，保留锁定账户和业务数据；有变动先停下核对。应用文件组回退不能代替outbox/短信状态回退；手动恢复旧Core时必须同步核对管理参数及Root配置。
