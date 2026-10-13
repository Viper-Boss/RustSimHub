# Wi-Fi 试连接与进程崩溃恢复实机验收

2026-10-14，P4 增加 2 点，总体约 56%。

源码 343cfab，源包 35ecbce247dd971c3e768e731cdbd3ec0b8cf1af8041c34583caa2fabb9307ee；实际 NAS workspace 131 项及默认 production core 构建通过。相同 Rust 源码的 1dfae42 CI 测试/格式/Clippy 成功：[运行](https://github.com/Viper-Boss/RustSimHub/actions/runs/38458209456)。此前 52746ad 的两个可合并 if 样式问题已修复，未忽略 lint。

| 产物 | SHA256 |
|---|---|
| 生产 core，fixtures=false | 0ba6e4ebbfc86c44b966c37b509daedecc03bee2e8437e67e30103bcf2da0db3 |
| 开发 core，fixtures=true | 4a3eb149a73620e9638b38fff3667354c6e75ea290000c12db35eac182215f07 |
| simhubctl | 3670332c431f257d5890cce03ecb1388ec407151aee7ab0501520cee53807529 |

核心专用 UID 986、空能力集合、没有设备访问。Root 操作端只保存自己的签名钥匙；实际网络操作每次核验 NetworkManage Principal/generation、NM/MM 唯一 owner、Wi-Fi 类型/UUID/SSID 身份、USB 已连接及全局没有活动通话。

- 只读 client 调用被拒绝，journal 没有新记录；owner 试连接原保存 Wi-Fi，恢复验证成功。
- 同一 key 在同进程及重启后返回原成功结果，只产生一个持久 intent/result，不重复激活。
- 开发构建专用 fault 模式先检查不存在可自动连接的蜂窝 profile。临时 profile 使用现有 WPA-PSK SSID、错误口令、IPv4/IPv6 disabled、never-default、volatile 和 dbus-client 生命周期；不是新开放热点或数据出口。
- 真实建立 Wi-Fi-only 8 秒 checkpoint，fault 激活成功返回后核心被 SIGKILL。没有 Core 进程继续补偿，NM 定时器恢复原 profile，Wi-Fi State=100。
- 原所有保存 profile 列表完全相同，临时 profile 已自动删除；USB 一直 State=100，WWAN 无线开关前后相同。没有调用普通 APN/WWAN 激活、互联网蜂窝探测或新电话/短信。
- fault key 在重启后保持拒绝，不重试不明操作；正常 key 仍缓存。默认生产构建没有 fault CLI。
- 临时 polkit 只向 Core 赋予本测试所需 NM 动作；三个项目账户 WWAN 控制及 Edge/Media 网络控制拒绝。验收后按确切 hash/UID/mode/nlink 移除自己的规则，Core network-control 恢复拒绝。旧四服务 active，PCM 全 closed，NM checkpoints=0。

私有证据 .private/wifi-artifacts-metadata.log、device-wifi-install.log、device-wifi-policy-trial.log、device-wifi-trial-first.log、device-wifi-policy-cleanup.log。协调器 tools/device-wifi-trial.sh；参考规则 firmware/development/wifi.rules 不自动安装。

边界：这验证有限试连接和故障恢复，还没有新 SSID 配网、热点、正式持久确认、远程管理或蜂窝预算/relay。进程终止不是物理掉电。持续目标 75% 未完成，百分比分母不变。
