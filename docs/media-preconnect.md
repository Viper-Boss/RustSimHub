# 拨号前预连接声音

2026-10-17，用户已确认0587cf7持续Chrome下行正常，首次接音仍约7秒、漏听开头。旧版本只提前做本地SDP/ICE；设备回答与DTLS仍在ACTIVE后。新流程源码已接通Core/Edge/浏览器，209 workspace、21 Native、两者Clippy及Node三组通过；冻结/ARM/实机验收尚待完成，不加进度。

用户一次点击后：麦克风授权/登录及旧操作处理 → 核电话空闲/线路就绪 → 浏览器与设备真实ICE/DTLS握手完成 → 固定10010准备/只提交一次 → 精确呼叫ACTIVE → 核当前租约/实际对端指纹、消费单次票据 → PCM与麦克风内容开启。预连接失败不拨号；拨号结果Unknown只查询，不重试。

边界与接口：

- Core内部CheckMediaPreflight仅核当前media.use/calls.control、principal/generation、boot、真实SIM线路与空闲或本principal控制中的呼叫，返回不外露的身份/线路fence。它不生成CallLease、媒体票据或PCM许可；泛用HTTP拒绝该命令。
- Edge媒体connect在同源/实际TLS peer/local和现有有界admission下，仅预留一个非PCM会话，固定session/offer/对端/fence，绝对时限30秒，不因poll延长。未激活时只能由同一当前授权身份poll/stop；激活时重新核完整真实呼叫lease/fence，之后永久切换为该呼叫绑定，不能换呼叫、peer或offer。
- Native现有Prepare已可无PCM完成真实握手；只在Authorize收到与实际DTLS指纹一致且Core消费成功的票据后打开ALSA。verified未授权等待有界增至30秒，仅限传输资源；不延长电话30秒租约、票据寿命或有效媒体断流边界。
- 浏览器预连接期间禁止上传麦克风内容，已有静音选择保留；取消/退出/权限撤销或超时立即停本地音轨和连接。server stop失败也不重发授权；设备资源独立有界回收。
- 记录本地准备、远端握手、拨号提交、观察ACTIVE、实际媒体ACTIVE的单调时长，不记音频、SDP或密钥。用实际接通到PCM/播放的时长验收，不承诺零延迟。

必须验证：预连接无呼叫/无PCM/无票据；缺scope/换principal或generation/换SIM/无效线路/另一owner占用拒绝；跨session/peer/offer拒绝，绝对到期不续；提前activate拒绝；不明authorize不再发票据；激活后旧preflight请求不能取代call lease；浏览器取消/迟到响应与麦克风静音；预连接失败零拨号。之后先NAS合成无PCM预连接，再独立保护下单次10010；真实浏览器听音仍需用户参与。

已跑本机回归覆盖无媒体scope/disabled/线路失效、零呼叫创建及零票据、外国owner/提前激活拒绝、Verified后仅一次切换到真实call fence、旧null lease不能访问已激活会话、绝对到期回收。浏览器测试确认握手完成后才准备拨号，预连接期间音轨disabled、mute(false)也不能上传麦克风内容；激活后复用同一个PeerConnection/offer/session，只有一次activate，没有第二次prepare；取消、过期、迟到回复、身份变化和预连接失败均关闭或阻断新拨号。Native阶段增加固定answer/verified/activation单调耗时日志；NAS无PCM合成端的私有开始标记等待同样有界30秒，方便先握手再拨号。以上测试不等于实机首句已能听全。
