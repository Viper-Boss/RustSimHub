# 网络状态、测试与月流量限制

2026-10-22新增。界面位于管理网页顶部。当前硬件验收与部署状态以HANDOFF和最新验证报告为准，本文描述实现边界，不代替实机报告。

## 状态和无线参数

- 普通`network_status`仍使用原`status.read`有限投影，不向已有客户端增加位置等信息。
- 新`network_diagnostics`需要`network.manage`，通过Core读取Root监测服务的限长、正确所有者/权限/单链接、当前boot且30秒内的快照。未启动、过期或权限不足不会显示成联网成功。
- 页面显示接口状态、IPv4/IPv6、网关、DNS、当前已连接普通蜂窝APN、运营商、注册与接入制式、信号百分比、驻留小区、物理小区、TAC、EARFCN、RSRP/RSRQ等可得字段。
- 支持/允许频段与当前驻留频段明确区分。当前LTE频段、上下行频率仅根据实际服务小区EARFCN推导，范围有限；缺值不猜测。带宽若ModemManager不提供则显示“设备暂未提供”。参考[ModemManager接口定义](https://github.com/linux-mobile-broadband/ModemManager/blob/main/introspection/org.freedesktop.ModemManager1.Modem.xml)。不启用GPS、不开放AT/QMI任意命令接口。
- 不输出SIM卡号、IMSI、IMEI、APN口令、会话token、私钥或原始D-Bus/CLI返回。小区信息只向网络管理权限开放。

## 计量与月限额

板级bam-dmux接口统计可能始终为0，不能作为有效流量计。本模块在独占的`inet rustsimhub_net`表中，用具名计数器计量普通wwan接口的IPv4/IPv6接收、发送，包含本机和转发流量；不刷新其他防火墙规则。实时速率由两次实际采样的差和时间差计算，页面保留最近60次采样曲线。

Root持久目录为`/var/lib/rustsimhub/rustsimhub-v1/system/network`（0700），文件0600、独占锁和原子同步替换。每张SIM以Root随机盐派生本地摘要分开统计，公开快照不输出摘要或盐；最多3200条日记录、保留400天。按Asia/Shanghai（UTC+8）自然月或用户指定重置日统计，1–31日有效，短月份取最后一天。改重置日重新汇总已有日记录，不直接清空已用量。

`set_network_policy`需要最近5分钟登录的所有者及`network.manage`。请求含固定格式request_id、enabled、limit_bytes、reset_day，可显式接受计量差异；不接受身份、路径、接口、命令或APN任意覆盖。Core仅写有限配置提示，不持有NET_ADMIN。提示只属于当前boot、15秒有效，Root保存消费ID后才应用。旧boot或过期提示不会恢复旧策略；保存限额不激活数据。

限额启用时，入/出方向共用一个内核quota，并在达到限额后仅断开本轮确认的NM普通蜂窝profile，不关闭radio、不删除SIM或语音/IMS配置。默认限制关闭。关闭限制不会自动重连；测试与断线也不会隐式从USB/Wi-Fi回退到流量。

Root服务只具有NET_ADMIN、有限文件写目录、AF_UNIX/NETLINK，只运行固定的nmcli/mmcli/nft命令，无shell和任意参数API。监测服务启动前应用已保存的普通数据守卫，NetworkManager依赖它；停止时先阻断数据再保存最终计数。异常掉电或计数回退导致限额启用时计量不确定，先阻断普通数据，所有者可核对后显式接受已保存统计值。账单仍以运营商为准：本机是IP层估算，丢包、协议层计费与最后一次检查点之间有差异，不能保证运营商计费字节绝不超额。

提示目录`/run/rustsimhub-network-input`为2750、Core:Root，配置文件0640；Root通过组权限读取，不授予读取所有私有目录的DAC能力。快照目录为2750、Root:Core，文件0640，仅Core可读取投影。

## 主动测试

Edge提供同Origin、Host、JSON与请求标记约束的`POST /api/network/v1/test`。测试前后均向Core验证`network.manage`会话；仅允许用户选择的已连接接口，蜂窝需本次显式同意，不创建连接。DNS UDP与HTTPS TCP均首次绑定所选接口及源地址，绑定失败即停止；不会退回系统默认出口。

- 连通性：固定`www.baidu.com/robots.txt`，最大64KiB。
- 下载样本：固定`speed.cloudflare.com/__down?bytes=1048576`，最大1.1MB。国内网络可能不可达；失败原样报告，不改证书校验或换线路重试。
- DNS答复限长和解析边界、目标公网地址、系统CA及准确主机名校验；不跟随重定向，不接受URL/主机/端口输入。
- 一次最多15秒，全局最多一个测试，至少间隔15秒，无自动重试。显示DNS、TCP、TLS、首字节耗时、样本字节和样本速率；样本速率不宣称为线路最大带宽。
- 刷新状态不主动访问互联网。登出/页面隐藏停止轮询，迟到回复不能恢复已清空的数据；前端不保存月账本或测试凭据。

自动测试入口：`cargo test --workspace --all-features --locked`、`node tools/test-web-network.mjs`，以及现有网页生命周期回归。冷启动、真实限额、掉电及实际网络用量需另附设备证据。
