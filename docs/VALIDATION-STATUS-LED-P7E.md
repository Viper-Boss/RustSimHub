# 三色指示灯与事件接线 P7E

2026-10-21。用户报告新系统灯完全不亮。只读检查确认gpio-leds驱动已加载，三个通道最大亮度为1；green:wan关闭，blue绑定Wi-Fi发送、red绑定mmc活动，没有系统状态控制服务。不是通过猜测GPIO或更换内核修复。

## 已发生的实机结果

初版固定绿灯heartbeat方案c4db80e仅提交源码，用户改定颜色含义后未部署，已由独立Rust程序替代。58a43de精确源码/ARM构建通过，binary、auto/off配置、systemd单元和仅Wi-Fi的NM配置四文件以新路径安装，发布前后逐项摘要核对、同目录原子发布和fsync通过。记录`.private/status-led-deployment-20261021-first.json` passed=true。服务enabled/active/running、空capability、ProtectKernelTunables=yes，写入仅三条固定LED目录，初次采样MemoryCurrent=1835008，NRestarts=0；此值不是长期内存/CPU基准。

8秒采样包含RGB=111和RGB=000，用户确认白色慢速亮灭正常。boot_id前后相同，没有重启、分区写入、业务初始化、新电话、短信或普通蜂窝；USB维护仍工作，Wi-Fi及普通gsm均未连接。开机启用已设置，尚不能称冷启动灯光已验收。

## 源码验证

- `cargo test --workspace --all-features --locked --offline -j 12`：222项通过，包含9项灯控策略/输入元数据、Core提示与SIM投影及新IPC拒绝路径；生产Core检查、format与Clippy通过。
- `node tools/test-web-indicator.mjs`：配置退出顺序、会话捕获、合并请求和失败恢复通过，无网络/硬件访问。
- Root系统工具140项中139通过，默认PATH缺GNU make使1项跳过；用此前验证的make路径另跑8项内核工具检查全部通过，包含该跳过项。没有把跳过写成通过。
- 精确日志：`.private/status-led-hooks-workspace-20261021-final-second.log`、`.private/system-tools-status-led-20261021-final.log`、`.private/system-tools-status-led-make-20261021-final.log`。较早测试兼容性/夹具失败报告保留，不替换为成功报告。

未读投影只读现有schema5；SIM切换后通话会先变reconciling，夹具确认重新观察前不能提醒。网页确认和配置动作仍走原认证IPC、既有权限及输入边界，不新增任意灯色/路径/命令API。灯语、时序与慢脉冲限制见[指示灯约定](status-led.md)。

## 仍需验收

尚未初始化新系统业务。当前阶段源码接线测试不证明实机网页退出、真实短信阅读/未接确认或实际升级联动已通过；蜂窝测试继续等换卡。新业务制品交付、业务启用及新系统冷启动仍属后续阶段。源码与ARM制品准确归属、最新灯控更新及暂停检查点见HANDOFF。
