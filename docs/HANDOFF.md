# RustSimHub 当前接手入口

2026-10-17 当前权威状态；历史原文已完整保留在 [交接归档](archive/handoff-before-oneclick-consolidation-20261016.md)。下述状态覆盖归档中的旧检查点，不把候选用户态当成新系统已经刷入。

## 目标与进度

- 持续目标为 100%；用户要求每次最终回复报告百分比。
- 固定P0—P7分母，当前 **73.5/100，约74%**。P0=6.5、P1=14.5、P2=15、P3=14.5、P4=7.5、P5=5、P6=6.5、P7=4。a15用户确认持续Chrome下行正常，P3+0.5；双向听音和首次接音延迟仍缺。见[进度刻度](PROGRESS-SCALE.md)，构建/诊断不加分。
- 当前 SIM 没流量：普通 APN、蜂窝互联网探测及 USB/Wi-Fi 失败后的隐式蜂窝回退全部禁止，等换卡并明确安排。电话需要的 WWAN/IMS 保留。
- 用户持续允许拨打 10010，不需逐次确认。只做独立标记的单次短通话、自动挂断、记录次数；Unknown 先核验，不自动重拨。其他号码或新短信不能推定授权。
- 内核/DTB 精确来源、完整新 OS 启动、回刷等门禁未解除；不写原始分区/GPT/NV/校准区，不产出可刷发布包。

## 当前真实浏览器验收

累计实际10010 **36次**。316005e/a17真实Chrome一键先完成DTLS再拨号，页面显示“观察接通后接音0.5秒”，Native持续约26.6秒/1331captured/sent/played；首句完整尚未明确人工确认。页面末尾“挂断尚未确认”，后台停机只读副本显示本次hangup submitted、精确call ended/observed，MM全部terminated；旧四服务active、PCMclosed、Voice撤回/拒绝、预览/独占守卫停止、Windows临时证书移除全部核验。报告.private/browser-a17-acceptance.json。前一a16自动NAS测试观察ACTIVE后1187ms开PCM、媒体20.657秒；无拨号Verified保持12秒/零PCM/无票据也通过。

a15/0587cf7用户确认持续Chrome下行正常，但初始约7秒漏句；P3此前只因该项加0.5，预连接改进不重复加分。a10—12已确定旧Go agent约15秒Hangup冲突，必须保留独占/设备独立恢复；a13原24秒目标未通过，原passed=false与adjudication均保留；a14未拨号，不能计数。a1—17实际标记都不可复用；a17自动撤销了a15留下的旧prepared挂断，无重复提交。

收尾源码已改为拨号提交起27秒开始一次挂断，给不变的30秒设备保护留3秒，取消挂断前的阻塞查询；随后最多3秒/6次只读核精确call，含请求挂起总超时与晚到回应退出。Node三组通过（含迟到ended、丢失挂断回复、始终active、查询挂起），尚未冻结构建和实机验收，不标已解决页面确认。

详细证据见 [浏览器验收](VALIDATION-BROWSER-MEDIA-P3I.md)。此前 Chrome 插件能列标签页但接管为 Debugger unattached，没有实际控制浏览器，也不绕过其他页面的访问阻止。

## 当前窗口、守卫和证书清理

主要私有记录 `.private/browser-preview-20261016-first.json`，保留同一设备身份、浏览器配对和预览证书。地址 `https://192.168.120.167:8448`；实际目录、各 unit、owner 私钥和 SSH 凭据只从私有文件读取，不提交或打印。当前程序来自 report.diagnostic_revision 与 media-current-build.json，不能运行旧 874/715 重启脚本。

- 当前 helper：`.local/start-codec-browser-preview.py`、`.local/collect-latest-media-closure.py`、`.local/collect-core-media-diagnostics.py`；a12已消费，禁止复用其once/watch脚本拨号。`tools/legacy_controller.py`管理旧Go控制器的有界暂停/先停Core后恢复，单一systemd unit防重入，保留原先inactive状态；仅供旧系统过渡测试，新OS不包含旧agent。
- 独立 root 观察器最多等十分钟，发现一次 outgoing 后 30 秒停止 Core，撤回精确哈希的临时 Voice 规则。它不主动拨号或重拨。浏览器自己的测试亦不续租 Core 30 秒控制租约。
- Native collector 的输出文件名仍是历史 `.private/browser-preview-71506f0-current-native.log`，内容是当前实际 PID；采集后复制为当前源版本文件，别误认为程序是 715。
- **本轮证书已直接移除，installed=false/removed=true。** 后续同一证书仍只有临时听音授权，结束直接运行`.local/cleanup-preview-trust.ps1`并核验；不要依赖旧后台清理进程。SHA-256 e4726db3ab982ee19661c9af85bf6ab08fd59b224d045e5de49913ed07404809；拇指纹61EB6A4F98B49290C939A2187B331098B38003A4。只能处理本次证书，不关闭证书检查。
- 结束执行 `.local/browser-preview-control.py cleanup`，再核验呼叫全 terminated、PCM全closed、旧 nassimhub-agent/ModemManager/NetworkManager/rmtfs active、临时 Voice 规则撤回且恢复拒绝。cleanup helper 本身不替代呼叫/规则核验，也不移除 Windows 证书。

## 当前源码与制品

当前部署构建源 `316005e1d9c83c6348a0342b52ccdb9eb0fd14a7`，115输入，归档SHA `97627e851738653c9dfbde880f6cae2fa6ec72566489ae1965cbd1944cd3207c`。GitHub main已精确保存该源。后续收尾改进尚未新构建，不把316制品重标为新源码。

- 本机WSL六生产ARM：`.local/wsl-artifacts-316005e1d9c83c6348a0342b52ccdb9eb0fd14a7`，209 workspace/退出0及全量Clippy通过。
- NAS固定无网络ARM容器Native：`.local/native-316005e/rustsimhub-audio-alsa`，5516704字节，SHA `2ecdad25761d8816769a62beff9cfbee5eb277e8bb2c3fc2f4bd0e94f78ab4e1`；21项/Clippy/release3m12s/退出0，日志`.private/native-316005e-build.log`。
- 严格TLS五网页与冻结Git资产一致，首次启动检查一次5秒read timeout，随后五项重查全部通过；不隐去该短暂启动超时。Core50/Edge22及浏览器proof/media/call-test三组通过。
- `.private/media-current-build.json`、默认`.local/source.tar.gz`及独立frozen 316归档指本轮构建。开发签名包`.private/boot-application-316005e`三个epoch已保留，签名seed仅本机；未安装新的正式开机组或完整OS。
- `.private/http-preconnect-idle.json`、`.private/headless-preconnect-a16.json`、`.private/browser-a17-acceptance.json`记录当前实证；Native日志`.private/browser-preview-316005e-a17-native.log`，Core日志同前缀-a17-core.log（包含旧历史，按PID和时间识别）。无录音/新短信/普通蜂窝测试/分区写入。
- 旧0587/c1099/981/13f/d2/715制品和开发签名包保留，正式Media开机组实证仍属于715。候选RootDirectory仍旧用户态，不能称完整新OS已刷入。

## 已完成但不可扩大的系统实证

- [P6K Media 开机组](VALIDATION-MEDIA-STARTUP-P6K.md)：715 的可信媒体初装、同一签名槽位/UID/Root/inode/空cap，真实Media SIGKILL整组回退、第二次Edge SIGKILL恢复预算耗尽整组停止通过。P6=6.5。
- 完整旧候选用户态 RootDirectory 仍为 `.private` 报告记录的 system-tree-ofY1ttXm/rootfs；不是新系统完整上电。旧P6J组合及离线 rootfs.img仍保留，所有刷写/发布门禁false。实机 NSHSTATE 初始化、实际维护公钥安装、完整开机/掉电/回刷闭环仍缺。
- [P0B 9008只读](VALIDATION-EDL-READ-P0B.md)：实际进入/完整3909091328字节主区读取/返回通过。原图只读文件系统检查失败，原图保留；独立 preen 派生副本两分区检查0，不等于原图一致或完整恢复成功。boot0/1独立备份保留。无活跃EDL等待器，当前不需要用户拔插。
- NAS兼容桥/旧库迁移、USB/Wi-Fi真实HTTPS与NAS程序化媒体、独立短信等实证见 PROGRESS.md 对应验证文档。其他SSID/首启热点、远程TURN、来电与真实双向听音、长期稳定/等功能性能对照等仍缺。

## 接手顺序与工具

先冻结/构建浏览器收尾改进并验证单次挂断及有界结束观察；再安排新的独立标记10010。预连接已实机完成，保持Core原30秒租约/单次票据/断流边界及拨号前零PCM；首句是否完整和真正双向听音仍需用户参与，不能从0.5秒页面指标推定。全部a1—17标记已结束/消费，不复用；当前无等待用户的测试窗口或临时证书。之后推进正式服务组合、首启/系统与恢复门禁；流量待换卡。

Python 使用 `C:/Users/wzs11/AppData/Local/Programs/Python/Python312/python.exe -X utf8`；私有依赖 `.local/python-deps`。SSH用 `tools/remote.py` 读取 `.private/nas.json`/device.json，严格主机钥匙固定，设备经NAS。主六程序强本机WSL user wzs11、持久cache/8路交叉编译；Native仍NAS，未完成本机Native SDK迁移。源先提交、package-source.py冻结；构建索引改变前确认无编译任务。GitHub使用精确非强制对象发布脚本，勿推缺workflow授权的 codex/recovery-ci-pending 历史分支。
