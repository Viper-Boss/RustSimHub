# 拨号前预连接声音

2026-10-17，316005e已冻结、六ARM生产程序/Native构建和实机预连接通过；209 workspace、21 Native、两者Clippy及Node三组通过。a17真实Chrome页面显示观察ACTIVE后接音0.5秒，设备传声约26.6秒；此前用户反映0587cf7首次接音约7秒。该指标不等于从运营商实际接通到扬声器首声的端到端测量，a19最新用户确认第一句话及后续完整、最终挂断成功，页面0.7秒；原a17记录保留，不重复增加进度。

用户一次点击后：麦克风授权/登录及旧操作处理 → 核电话空闲/线路就绪 → 浏览器与设备真实ICE/DTLS握手完成 → 固定10010准备/只提交一次 → 精确呼叫ACTIVE → 核当前租约/实际对端指纹、消费单次票据 → PCM与麦克风内容开启。预连接失败不拨号；拨号结果Unknown只查询，不重试。

边界与接口：

- Core内部CheckMediaPreflight仅核当前media.use/calls.control、principal/generation、boot、真实SIM线路与空闲或本principal控制中的呼叫，返回不外露的身份/线路fence。它不生成CallLease、媒体票据或PCM许可；泛用HTTP拒绝该命令。
- Edge媒体connect在同源/实际TLS peer/local和现有有界admission下，仅预留一个非PCM会话，固定session/offer/对端/fence，绝对时限30秒，不因poll延长。未激活时只能由同一当前授权身份poll/stop；激活时重新核完整真实呼叫lease/fence，之后永久切换为该呼叫绑定，不能换呼叫、peer或offer。
- Native现有Prepare已可无PCM完成真实握手；只在Authorize收到与实际DTLS指纹一致且Core消费成功的票据后打开ALSA。verified未授权等待有界增至30秒，仅限传输资源；不延长电话30秒租约、票据寿命或有效媒体断流边界。
- 浏览器预连接期间禁止上传麦克风内容，已有静音选择保留；取消/退出/权限撤销或超时立即停本地音轨和连接。server stop失败也不重发授权；设备资源独立有界回收。
- 记录本地准备、远端握手、拨号提交、观察ACTIVE、实际媒体ACTIVE的单调时长，不记音频、SDP或密钥。用实际接通到PCM/播放的时长验收，不承诺零延迟。

必须验证：预连接无呼叫/无PCM/无票据；缺scope/换principal或generation/换SIM/无效线路/另一owner占用拒绝；跨session/peer/offer拒绝，绝对到期不续；提前activate拒绝；不明authorize不再发票据；激活后旧preflight请求不能取代call lease；浏览器取消/迟到响应与麦克风静音；预连接失败零拨号。之后先NAS合成无PCM预连接，再独立保护下单次10010；真实浏览器听音仍需用户参与。

已跑本机回归覆盖无媒体scope/disabled/线路失效、零呼叫创建及零票据、外国owner/提前激活拒绝、Verified后仅一次切换到真实call fence、旧null lease不能访问已激活会话、绝对到期回收。浏览器测试确认握手完成后才准备拨号，预连接期间音轨disabled、mute(false)也不能上传麦克风内容；激活后复用同一个PeerConnection/offer/session，只有一次activate，没有第二次prepare；取消、过期、迟到回复、身份变化和预连接失败均关闭或阻断新拨号。Native阶段增加固定answer/verified/activation单调耗时日志；NAS无PCM合成端的私有开始标记等待同样有界30秒，方便先握手再拨号。以上测试不等于实机首句已能听全。

实机证据：

- `.private/http-preconnect-idle.json`：严格HTTPS/真实DTLS Verified保持12031ms，全部媒体计数0、PCM关闭、无CallLease/票据/拨号；泛用API预检403、伪造token403、提前activate400。原四服务保持，清理通过。
- `.private/headless-preconnect-a16.json`：拨号前握手3125ms，观察ACTIVE后PCM启用1187ms，媒体20657ms；NAS解码1048帧、静音上行、无NAS音频设备或录音。独占控制/独立30秒停止及原服务恢复、Voice撤回/PCM关闭通过，10010累计35。
- `.private/browser-a17-acceptance.json`：真实Chrome一键，显示0.5秒接音；Native握手answer1525ms/verified43ms，真正PCM activation141ms；1331采集/发送/播放，1339接收、26丢弃、1次可恢复欠载。约26.6秒后结束；页面提示挂断尚未确认。停机后只读数据库显示该次hangup submitted、精确call ended/observed，MM全部terminated，独占守卫/预览停止、旧四服务恢复、PCM关闭、Voice拒绝、Windows临时证书已移除。10010累计36，不能把后台清理成功冒充当时页面已确认。

收尾改进4e6fea6已冻结/构建：浏览器在提交拨号起27秒启动一次挂断，给原30秒设备保护预留3秒；保留单次submit/不续租、不重拨。挂断后最多3秒/6次只读观察精确call，包含请求挂起的总超时，晚到查询不再启动新查询。Node回归验证迟到ended、挂断回复丢失、始终active和查询挂起均不再次提交，麦克风立即关闭。六生产ARM/209 workspace、Native21和Node三组通过。

a18以本机Node执行冻结的相同CallTest JS，经真实严格HTTPS/已核签owner/独占控制和独立30秒保护拨打一次10010：一次dial/一次hangup、未续租，精确call ended/observed及最终“测试已结束，电话已挂断”通过，28.654秒。该用例音频对象为stub，完全不测试PCM/麦克风/实际声音；其0.0秒显示值不可当作媒体延迟。退出MM全部terminated、PCMclosed、Voice拒绝、旧四服务active、所有预览/独占守卫停止、证书保持已移除，.private/call-cleanup-a18.json passed/cleanup_verified=true。累计10010=37；真实Chrome新收尾和首句完整仍缺人工确认，不重复加进度。


## a19真实Chrome最终验收

2026-10-17，4e6fea6/a19真实Chrome一键听音：用户明确“现在所有的声音都听完整了”，并反馈“测试已结束，电话已挂断；观察接通后接音0.7秒”。Native约24.26秒传声，1212captured/sent/played、1220received、19dropped、1次可恢复xrun；answer1493ms/verified40ms/activation141ms。停机后只读一致数据库确认本次dial/hangup均submitted且同一精确call ended/observed；MM全部terminated、PCMclosed、Voice撤回/拒绝、旧四服务active、预览/独占守卫停止、Windows临时证书直接移除并核验。私有browser-a19-acceptance.json与browser-preview-4e6fea6-a19-native.log保留，累计10010=38。0.7秒是页面观察ACTIVE至媒体启用，不是运营商接通至扬声器首声的端到端测量；尚未验收双向听音。无新短信/普通蜂窝测试/分区写入。

持续下行此前已计入P3，本次首句/收尾闭环不重复计点；整体73.5/100约74%。
