# RustSimHub 当前接手入口

2026-10-16 当前权威状态；历史原文已完整保留在 [交接归档](archive/handoff-before-oneclick-consolidation-20261016.md)。下述状态覆盖归档中的旧检查点，不把候选用户态当成新系统已经刷入。

## 目标与进度

- 持续目标为 100%；用户要求每次最终回复报告百分比。
- 固定 P0—P7 分母，当前 **73/100，约 73%**。P0=6.5、P1=14.5、P2=15、P3=14、P4=7.5、P5=5、P6=6.5、P7=4。见 [进度刻度](PROGRESS-SCALE.md)。构建、诊断和重复运行不额外加分。
- 当前 SIM 没流量：普通 APN、蜂窝互联网探测及 USB/Wi-Fi 失败后的隐式蜂窝回退全部禁止，等换卡并明确安排。电话需要的 WWAN/IMS 保留。
- 用户持续允许拨打 10010，不需逐次确认。只做独立标记的单次短通话、自动挂断、记录次数；Unknown 先核验，不自动重拨。其他号码或新短信不能推定授权。
- 内核/DTB 精确来源、完整新 OS 启动、回刷等门禁未解除；不写原始分区/GPT/NV/校准区，不产出可刷发布包。

## 当前真实浏览器验收

累计实际 10010 **28 次**，a8已实际拨出并被独立保护挂断。981实机约12.5秒媒体后Native Denied、Edge core_lease/HTTP403；用户挂断未确认，实际全部call terminated，预览已停止/旧四服务active/PCMclosed。详见验证文档新增第八标记。下一步Core固定拒绝原因诊断及无PCM合成端20秒自动通话，暂时不再让用户试听。

1. 715 版本在约 4—5 秒因 Codec 退出。d2b1c13 加诊断并修正晚到/重复包，实机准确抓到 `packet_guard bytes=0` 触发 Codec 退出。
2. 13f9063 修正已认证空负载安全丢弃，不入 Opus、不消费序列、不刷新有效媒体存活时间。新增顶部一键 10010、自动登录/拨号/接音、约 30 秒结束、立即结束、旧操作防重和麦克风取消；手动步骤收进折叠区。
3. 用户已实际通过一键操作完成通话全流程，但声音约 13 秒就停止，提示“测试停止：声音提前中断；电话已挂断”。Native 639 captured/sent/played、646 received、13 dropped、1 xrun，无 Codec 拒绝；不是 30 秒保护正常结束。已保存 `.private/browser-preview-13f9063-current-native.log`。
4. 当前 9812aa5 修正可复现的 reaper/HTTP slot 锁竞争：有界等待 1500ms；授权/归属仍逐次检查；只读 poll 的 429 最多三次查询，prepare/activate/拨号/挂断均不重试，403立即停音。增加固定 stop/拒绝原因及页面 HTTP 状态。旧现场没有 HTTP 状态，不能认定竞争就是该次现场原因。
5. 981 已部署并严格 TLS 核对五个页面响应 SHA/no-store。已让用户刷新并只点“一键测试 10010 · 30 秒”；当前等待本次结果，不要再次要求准备/提交/连接声音。持续声音仍未验收，不加 P3 分。

详细证据见 [浏览器验收](VALIDATION-BROWSER-MEDIA-P3I.md)。此前 Chrome 插件能列标签页但接管为 Debugger unattached，没有实际控制浏览器，也不绕过其他页面的访问阻止。

## 当前窗口、守卫和证书清理

主要私有记录 `.private/browser-preview-20261016-first.json`，保留同一设备身份、浏览器配对和预览证书。地址 `https://192.168.120.167:8448`；实际目录、各 unit、owner 私钥和 SSH 凭据只从私有文件读取，不提交或打印。当前程序来自 report.diagnostic_revision 与 media-current-build.json，不能运行旧 874/715 重启脚本。

- 当前 helper：`.local/observe-browser-audio-eighth.py`、`.local/collect-latest-media-closure.py`、`.local/collect-poll-edge-diagnostics.py`。
- 独立 root 观察器最多等十分钟，发现一次 outgoing 后 30 秒停止 Core，撤回精确哈希的临时 Voice 规则。它不主动拨号或重拨。浏览器自己的测试亦不续租 Core 30 秒控制租约。
- Native collector 的输出文件名仍是历史 `.private/browser-preview-71506f0-current-native.log`，内容是当前实际 PID；采集后复制为当前源版本文件，别误认为程序是 715。
- **临时 Windows 证书当前仍 installed=true、previously_trusted=false、remove_after_test=true。连续测试结束或暂停必须直接执行 `.local/cleanup-preview-trust.ps1` 并核验 removed=true/installed=false、当前用户 Root 中不存在。不要依赖旧后台清理进程。** 证书 SHA-256 为 e4726db3ab982ee19661c9af85bf6ab08fd59b224d045e5de49913ed07404809；拇指纹61EB6A4F98B49290C939A2187B331098B38003A4。只能处理本次明确授权证书，不关闭证书检查。
- 结束执行 `.local/browser-preview-control.py cleanup`，再核验呼叫全 terminated、PCM全closed、旧 nassimhub-agent/ModemManager/NetworkManager/rmtfs active、临时 Voice 规则撤回且恢复拒绝。cleanup helper 本身不替代呼叫/规则核验，也不移除 Windows 证书。

## 当前源码与制品

构建源 `9812aa511aba7c39bee971912dfe7afe44434e1e`，115 输入，归档 SHA `6fa7a8a5ecf929ec10fba8a263a26f2f991e7f6cdab1a1c35a9597ff4bf33cc6`。GitHub main 已精确保存该源码；之后文档提交若改变 HEAD，不能把制品重标为新 HEAD。

- 本机 WSL Rust1.98.0 六个生产 ARM64 程序 `.local/wsl-artifacts-9812aa511aba7c39bee971912dfe7afe44434e1e`，退出0。
- NAS 固定无网络 ARM 容器的 Native Rust1.99.0/ALSA1.2.8：`.local/native-9812aa5/rustsimhub-audio-alsa`，5516704字节，SHA `612bd66267747848cfe9ac7467dd4718d858bcfaa197700ba370f637a87e87d7`，21项、release3m11s、退出0。完整日志 `.private/native-9812aa5-build.log`。
- Edge 20 项定向回归及 Clippy 通过，日志 `.private/media-poll-contention-source.log`；Node proof/media/call-test 通过，额外验证 active 状态的短暂429不关麦、403立即关闭和激活不重试。此前全 workspace202、Native21及Clippy通过；不把定向20项冒充新跑全workspace。
- `.private/media-current-build.json`、默认 `.local/source.tar.gz` 和独立 frozen 981 归档均指当前构建。没有活跃构建；当前预览已停止，准备Core诊断版本。
- `.private/boot-application-13f9063`、d2b1c13、71506f0 开发签名包分别保留，seed只在本机；当前981开发签名包已保存在.private/boot-application-9812aa5。正式Media开机组实证属于715，不能重标当前981。

## 已完成但不可扩大的系统实证

- [P6K Media 开机组](VALIDATION-MEDIA-STARTUP-P6K.md)：715 的可信媒体初装、同一签名槽位/UID/Root/inode/空cap，真实Media SIGKILL整组回退、第二次Edge SIGKILL恢复预算耗尽整组停止通过。P6=6.5。
- 完整旧候选用户态 RootDirectory 仍为 `.private` 报告记录的 system-tree-ofY1ttXm/rootfs；不是新系统完整上电。旧P6J组合及离线 rootfs.img仍保留，所有刷写/发布门禁false。实机 NSHSTATE 初始化、实际维护公钥安装、完整开机/掉电/回刷闭环仍缺。
- [P0B 9008只读](VALIDATION-EDL-READ-P0B.md)：实际进入/完整3909091328字节主区读取/返回通过。原图只读文件系统检查失败，原图保留；独立 preen 派生副本两分区检查0，不等于原图一致或完整恢复成功。boot0/1独立备份保留。无活跃EDL等待器，当前不需要用户拔插。
- NAS兼容桥/旧库迁移、USB/Wi-Fi真实HTTPS与NAS程序化媒体、独立短信等实证见 PROGRESS.md 对应验证文档。其他SSID/首启热点、远程TURN、来电与真实双向听音、长期稳定/等功能性能对照等仍缺。

## 接手顺序与工具

先收当前 a8 一键测试结果与 Native/Edge 退出日志，核对实际呼叫次数，结束后执行两处清理并保存进度。不要在 Unknown 时重复拨号，不把13秒听音记为稳定。根据实际HTTP原因继续修复；若通过，只按已验收边界加分。

Python 使用 `C:/Users/wzs11/AppData/Local/Programs/Python/Python312/python.exe -X utf8`；私有依赖 `.local/python-deps`。SSH用 `tools/remote.py` 读取 `.private/nas.json`/device.json，严格主机钥匙固定，设备经NAS。主六程序强本机WSL user wzs11、持久cache/8路交叉编译；Native仍NAS，未完成本机Native SDK迁移。源先提交、package-source.py冻结；构建索引改变前确认无编译任务。GitHub使用精确非强制对象发布脚本，勿推缺workflow授权的 codex/recovery-ci-pending 历史分支。
