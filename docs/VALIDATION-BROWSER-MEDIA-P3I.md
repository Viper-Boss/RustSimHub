# 浏览器媒体拒绝分类与晚到包修正

2026-10-16，本轮固定源码 d2b1c13431213a65dd47023ef0e4182b82ade3e2，源归档 SHA-256 3399bf9bb70402ceb6cd17cd44ae851b704e96eca81bf16bf97fe8a416569fa9。本机六个生产 ARM 程序与 NAS 固定镜像 Native ARM 构建均退出 0。Native 为 5516704 字节，SHA-256 e45cb9bf35feeb31fcdfdb7eabc495177772bdb77b872ea79e2bcaab482769b7；20 项 Native 测试与 Clippy 通过。本轮重新逐个核对六个主程序及 Native 的本机文件摘要和源归属。

上一版 71506f0 的浏览器现场在约 4—5 秒出现 `worker_rtc_failure stage=service kind=Codec`，不能认定是 30 秒保护挂断或欠载。新版本在原有包长、年龄、授权和格式边界内，细分 packet_guard、packet_parse、duration、opus_decode、decoded_length 和 media_shape；日志只含拒绝类型、布尔值及长度，不保存语音、SDP 或密钥。

另发现原接收序列检查会将晚到或重复的认证包当作致命错误。现在这些包在 Opus 解码前丢弃，不入播放队列、不刷新有效媒体存活时间；过期、未来、超长、错授权、错格式的拒绝边界保持。回归覆盖重复/更旧序列丢弃后后续合法序列仍可解码，以及撤销优先。此修正尚不能当作现场中断原因已查明。

部署复用同一预览身份、配对和证书；三个服务停止后才逐个校验并原子替换可执行文件，再检查 build-info。旧版本制品与私有证据保留。新的三个 epoch 开发签名包位于 `.private/boot-application-d2b1c13`，签名 seed 只在本机；不替换已验证的 715 开机组证据或旧完整系统组合，不是生产发布。

实际浏览器结果、呼叫计数及清理状态以本轮最终交接记录为准。在新结果出现前，连续声音仍未验收，整体保持 73/100，不以构建、诊断日志或独立序列修正加分。没有执行普通蜂窝数据测试或分区写入。

## 本轮实机结果与一键测试源码

第五个观察标记等待超时，没有拨号；第六个标记实际拨出一次，累计 10010 从 25 到 26。用户报告约五秒后“声音连接失去确认，麦克风已关闭”。Native 在 12:25:10.812 UTC 明确记录 `codec_rejected reason=packet_guard bytes=0 future=false stale=false nonincreasing_sequence=false`，随后 Codec 退出；253 captured/sent/played、261 received、8 dropped、1 次可恢复 xrun。用户同一通话再次连接声音，第二次在独立 30 秒保护停止 Core 后因授权撤回结束；不是第二次拨号。全部呼叫 terminated，预览已停止，旧四服务 active、PCM closed；原始日志保存在私有 d2b1c13 文件。

本轮新源码将已认证、已通过媒体格式和时间/授权检查的零长度负载分类为 EmptyDiscarded：不调用 Opus、不消费接收序列、不入播放队列、不刷新有效媒体存活时间。新增回归证明超大序号空包不阻断下一个合法包、空包过期仍拒绝、撤销后空包不能通过。SRTP 剥离 padding 后可无媒体内容，与 [WebRTC 官方 RTP 说明](https://webrtc.googlesource.com/src.git/+/8188e5415cc122fbfe35b02c4c41bebc5561bf54/pc/g3doc/rtp.md) 和 [RFC 3550 第 5.1 节](https://www.rfc-editor.org/rfc/rfc3550.html#section-5.1) 的 padding/媒体区分一致；本轮未保存原始包，因此不把具体 padding 探测来源当成已捕获证据。

用户要求简化页面：新增顶部“一键测试 10010 · 30 秒”和“立即结束”，连接/配对及手动操作收进折叠区。一次点击先获取麦克风权限，再使用原批准身份签名登录，查询旧操作与通话空闲，固定号码准备/持久化提交标记/仅一次拨号，按当前精确 call_id 等待 active 后自动接入声音。倒计时结束或声音中断时立即关闭麦克风并只提交一次挂断；页面不续租测试通话，Core 原 30 秒租约为浏览器退出/卡顿后的独立清理约束。Unknown/提交回复丢失只查询且阻止再拨；新点击不会覆盖结果不明的旧操作。

Node 三组验证（proof、media 生命周期、一键通话编排）与 JS 语法检查通过。编排模拟验证重复点击只拨一次、固定 10010、拨号前拒绝权限/忙线、提交前持久化标记、丢失回复后不再拨、迟到权限与提交回复不恢复声音、倒计时挂断且没有 renew_call。WSL 202 项 workspace、21 项 Native 与两者 Clippy 均通过。新源码须冻结、ARM 构建、部署与真实一键听音验收；不提前认定五秒问题已解决，不增加进度。

## 一键页面部署检查点

冻结源码 13f906377ab4f63e0ab165a976ab257167cf877c，115 个输入，归档 d4644bb15b64319b9824e89872c3bd4a175e016ca44d5b9540d42468f174b492。强本机六 ARM 程序与 NAS 同源 Native ARM 构建均退出 0；Native 21 项、release 3m16s，5516704 字节、SHA 8dbc2e48601fb08ee9c44232e52a658429a5658e764928b3d04528e7dbfa852e。Native build.json 和完整日志留在私有区域；GitHub main 已精确发布本提交。

停止三个预览服务后，核对实际旧 d2b1c13 摘要及本机保留制品，原子替换 Core/Edge/Native，逐个 build-info 归档一致。复用同一身份、配对和 TLS 证书重启后，通过 NAS 严格 TLS 验证 /、/app.js、/media.js、/call-test.js、/style.css 五个实际响应均与冻结 Git 内容 SHA 一致且 no-store，报告 .private/browser-oneclick-assets.json passed=true；这证明页面已部署，不等于浏览器点按或持续声音通过。

第七个独立标记已准备一个十分钟等待窗口，用户只需刷新并点顶部按钮；观察到实际拨号前累计仍 26。约 30 秒独立 Core 停止保护仍保留，不会主动拨号或自动重拨；最终结果与清理记录须在后续交接更新。

## 首次真实一键结果：仍提前结束

第七标记实际拨出一次，累计 27。用户只需点顶部按钮，已完成自动登录、准备/提交拨号、自动连接声音、提前关闭后自动挂断；最终提示“测试停止：声音提前中断；电话已挂断”。实际拨出 12:47:44 UTC，PCM 12:47:47.18 开始，12:47:59.98 关闭，约 13 秒媒体；独立保护在观察后 30 秒才停止 Core。Native captured/sent/played639、received646、dropped13、xrun1，未重现 Codec 拒绝，`worker_closed failure=None`，此前先见 transport Closed。用户估计约15秒。不是30秒倒计时正常结束；持续声音仍未验收，不加分。原始日志 .private/browser-preview-13f9063-current-native.log 保存；服务已停止、旧四服务 active、PCM closed、呼叫全 terminated。

源码另发现可复现的轮询竞争：后台 reaper 持 slot 锁检查 worker 时，HTTP 的 try_lock 会直接429；原浏览器监视对所有非200立即停音。新 Edge 在原有有界 HTTP admission 内最多等锁1500ms，再执行完整 Core 授权/归属检查，超时仍429。新增回归验证短暂检查不拒绝本 owner poll、外来 binding 等待后仍403、持续占用有界返回429且不触发 worker 或新票据。浏览器仅对只读 poll 的429最多三次（100/200ms间隔）查询；prepare/activate/拨号/挂断不重试，403立即关闭。增加不含凭据的固定拒绝/stop原因，绿色提示保留HTTP状态。由于旧现场没有HTTP状态日志，此竞争尚不能被认定为这一次现场退出的确定原因。

## 第八标记：HTTP403 与 Core 许可失效

9812aa5 / 源6fa7a8a5…本机六 ARM 与同源Native21项/5516704字节、SHA612bd662…/release3m11s均退出0，已严格TLS五资产核对并实测。第八标记实际拨出一通，累计28；观察13:06:31 UTC，PCM13:06:34.73打开、13:06:47.26退出（约12.5秒媒体），623captured/sent、631received、625played、14dropped、1xrun。Native failure=Denied；Edge13:06:47.30明确core_lease拒绝。用户绿色提示包含HTTP403，挂断回复未确认；独立保护13:07:02停止Core后基带所有呼叫terminated，旧四服务/PCM恢复检查通过，原日志分别保存981私有Native/Edge文件。没有再现Codec；不能把锁竞争修正称为403修复。

当前补 Core 的固定拒绝原因、是否仍持有呼叫、剩余租约毫秒和binding_valid，仅写私有日志、不含号码/钥匙/媒体内容；没有放宽授权。合成无PCM端允许最长30秒，以自动做20秒长通话重现；AnswerAuthorized硬件诊断仍最多10秒。接下来先自动测试，不再要求用户反复听音；来源原因尚未确定，整体仍73/100。
