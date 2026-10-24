# P4L Root热点试连请求与执行循环

精确源码f255136的[CI40068753797](https://github.com/Viper-Boss/RustSimHub/actions/runs/40068753797)两个任务全部成功，含真实材料CLI步骤；私有完整证据configurable-ap-ci-20261024-first.json。候选未部署。

最终本地345项工作区Rust（setup-ap 57项）、严格Clippy、Core生产检查、三个实际材料CLI及三程序/库ARM64通过；精确提交CI待单独核对。私有configurable-ap-full-20261024-second.log/.exit、configurable-ap-arm-20261024-first.json为最终本地结果。

2026-10-24。Root新增显式`--serve-configurable`，接入受保护的STA试连/取消文件、持久防重、执行进度和返回AP后的真实HTTPS复核。原`--serve`路径保持默认行为。尚未接Core真实Owner命令生产者、网页或最终保存；不宣称完成独立配网。原P0–P7进度保持79.5%，业务仍是此前确认的185ec6e/43。

```text
rustsimhub-setup-ap --serve-configurable PRIVATE_CONFIG CORE_INPUT ROOT_OUTPUT ROOT_JOURNAL
```

该命令仅Root，沿用真实材料/证书/设备身份检查和固定目录UID/GID/权限。没有安装或启用服务模板，也没有把新入口加入现有业务启动。构建/执行夹具不代替棒子实测。

## 请求和响应

新增固定槽`station-request.json`及`station-status.json`，仍使用Core输入目录2750/Core:Root、Root输出目录2750/Root:Core、640文件、大小上限8192、拒绝链接及严格元数据。并未增加网络请求端点。

`channel/station.rs`的Request绑定完整原AP Binding、独立trial_id、恢复主体公钥摘要/generation、请求和截止时间。只允许Test（SSID和受保护口令）或Cancel，没有Save、设置字典、设备路径、shell或蜂窝操作。Test要求请求15秒内、原AP至少剩余60秒、试连期限最多40秒，且为原AP清理留余量。取消要求精确原AP/主体/generation/trial。口令类型没有Debug/Clone，解析缓冲使用Zeroizing；状态、intent和终态回执不含SSID或明文口令。

这些主体字段只是**可信本地Core通道的声明，不是独立的Owner认证证明**。后续Core必须由真实当前Owner、最近登录、network.manage及原AP会话授权派生该声明，不能接受前端提交的owner字段直接透传。当前没有Core生产入口或公共API，不能把Root夹具的合成主体当作已完成真实鉴权。

## 执行及恢复

Root先消费原AP意图，固定隔离安装后启动RAM AP；对每笔试连先同步无口令intent，再发Testing，最后才创建内层checkpoint/激活STA。最多16笔试连且不续原AP截止时间。使用P4K的同一无线设备、独立45秒checkpoint和有限返回事务。

试连期间暂停AP-only可达判断，继续核对原Root租约，并更新主AP Starting及试连Testing。收到精确取消、租约失效、停止信号或发布失败时发送内部取消，仍等待试连的有界恢复；不直接drop带清理责任的future。返回后核验AP HTTPS，再次检查隔离和Root租约，才发布Ready或Failed。网络恢复但HTTPS失败、或HTTPS检查期间权限/租约变化，结果为Unknown。

同一trial_id的同一请求只读取已持久终态，不再次激活；更改口令/SSID等导致摘要不同时拒绝。只有intent而没有终态时保持不确定，绝不自动重放。Root进程恢复仍先处理父AP pending为Unknown，不从文件重建活动试连。

退出时继续尝试精确整段AP回退；只有原AP资源清理和固定隔离释放都证明完成，才写父AP Closed并清除父pending。试连未知且无法证明清理时保留pending/隔离。试连结果未知但后续整段AP清理已证明完成时，可关闭父AP，不能因此把试连结果改成成功。

## 验证

最终本地/ARM及精确提交CI结果另记在本页补充与私有checkpoint。新增Root循环夹具覆盖请求一次执行、重复读取终态不重放、精确主体取消、错误主体取消拒绝、停止/租约过期时等待阻塞试连清理、恢复未知保留隔离、返回HTTPS失败、HTTPS探针期间授权变化和同ID改载荷拒绝。

协议测试覆盖新鲜度/原AP余量/40秒上限、错误绑定、额外字段/任意设置、弱口令/未来时间、取消主体以及HTTPS/有效AP回执。Journal测试证明原父会话要求、口令不入intent、重启不重放、改摘要拒绝和终态不可覆盖。仅有一次测试模块可见性编译失败，修正后完整回归；失败日志保留。

本轮另按用户明确授权准备日常网页SMS验收：只读SSH/SFTP基线有原67条短信和无发送中操作，十分钟观测没有新SMS操作；用户被浏览器旧通话待核对记录拦截，未执行短信发送。读取浏览器插件失败，随后窗口访问审批超时，没有UI动作成功；已请用户只查询通话操作，不清除防重记录。此独立实机只读观测不属于Root候选安装或BLE探测。

随后用户明确授权代理直接调试浏览器。插件附加/新建调试页多次超时（新页是否创建未确认），窗口访问先超时，最终工具因无法可靠确认当前浏览器URL而终止本轮电脑操作；未确认任何业务按钮动作成功，不再尝试其他UI技术绕过。用户返回的是设备状态，不是浏览器pending操作ID；页面旧通话待核对仍未解决。原一次10010/CXYE授权尚无发送证据，后续先核对旧操作并重新准备有界只读观测，不自动重拨/重发。

没有交付本轮候选、开启热点、切网、拨号、代理发送短信、启用普通数据或改证书信任；累计10010仍44。Root候选完全未在棒子运行。

## 下一步

1. Core真实Owner/network.manage与最近登录、原AP会话/主体绑定的有限试连命令；Root能力标志及受保护请求/结果查询，保留原Core网络锁。不能绕过旧USB Wi-Fi门禁或假造Owner。
2. 返回热点后可读的试连结果，再接用户明确确认、持久保存与最终STA启动。仍缺无SIM首配、已认领但未配网时重启的恢复、网页及联合签名安装。
3. 用户新增BLE应急配网与Web Bluetooth任务书，作为独立扩展队列和独立开发分支，不能用本次AP本地协议代替BLE身份、加密或无USB通道证明；真机BLE按任务书另行授权，原79.5%不混入BLE进度。
