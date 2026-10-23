# 正式业务升级后的独立冷启动验收

后续启动日志补充：本次冷启动早期有媒体依赖和Edge预检查失败，随后系统自动整组恢复并于13:26可用；原验收证明最终独立自恢复/听音成功，不证明首次启动无失败。恢复标记保持，本boot一次预算已经消耗；详见[P3L交付说明](VALIDATION-ONE-CLICK-INCOMING-P3L.md)，启动时序仍待修，不能据此宣称长期稳定。

2026-10-23。P6进展8.5→9/12，整体78→78.5%。本轮完成正式应用、身份与业务数据的普通冷启动和真实下行声音闭环，不代表完整镜像OTA、掉电回选或双向说话已经验收。

## 实际启动与数据

沿用原发布信任的应用提交 `e0f843babcc566071af18d3c033cfd1c0b375e36`，schema6、floor/confirmed25、candidate为空。独立Root网络组件仍0d56647，Root音频平台998d48f，灯控0c817c1；整组程序摘要、原factory16和旧confirmed22文件均保持。没有重跑初始化、刷分区、写NV/校准、触发关机驱动或重启基带。

用户普通拔插前停止业务并制作新的245760字节一致性业务备份，SHA-256 `cf225e91400310795f5d2315c96af26e77a4b5e3f8f753aba1b54566229776ce`；远端/本机读回及数据库完整性已核对。600秒业务恢复计时器只用于拔插前现场；新启动后不依赖该计时器。

新boot ID与拔插前不同。先通过原保存Wi‑Fi和严格固定SSH身份读取，现场NAS USB网口没有IPv4地址，设备严格HTTPS提供精确新资源。之后仅激活NAS原有不自动连接的USB静态配置，未修改配置或默认路由。这个证据证明当前保存Wi‑Fi场景下业务不依赖NAS USB维护地址，不证明无保存SSID首次AP或远程访问。

Root音频启动单元真实执行一次，七项路由从0/7初始化到7/7，未打开PCM。rmtfs使用原私有只读profile，Rust读取PS语音偏好为3，SIM注册home。正式Core/Edge/Media及网络守卫正常运行；身份、唯一active Owner、已消费的设置标记、TLS/媒体配置和原应用账本逐项保持。

原43条收件箱记录逐ID/内容保留，其中包含此前独立核实的3条实际新收短信。本次跨启动新增23条带duplicate_candidate的重观测，共66行；逐条确认同SIM候选、原记录存在且序号更早、内容/摘要相同。它们不是新收23条短信，也未自动合并。原Unknown操作完整保留。

## 冷启动后的真实声音

用户仅点一次10010一键短测，本轮实际新增一通，累计44。后台观察到dialing→ringing-out→ACTIVE→terminated、PCM实际运行。用户反馈“完整；测试已结束，电话已挂断；观察接通后接音0.4秒”。

媒体约25秒，captured/sent/played各1246，received1253，dropped20、xruns1。RTC关闭与浏览器明确停止同秒，随后真实电话terminated、所有PCM关闭；服务NRestarts均0。保留少量丢弃/一次可恢复欠载，不声称零故障或长期稳定。收发计数只证明媒体链路交换，不证明真实对端听到麦克风讲话。

本轮沿用用户已授权的同一证书临时信任，结束后核实移除；未关闭证书或主机名校验。普通gsm始终disconnected、无普通gsm连接配置，未做蜂窝互联网探测、发送短信或保存录音。

## 新短信页面

用户打开带候选的短信并显式读取旧记录，反馈两栏/正文正常；提供的正式页面截图显示当前和候选记录并排，中文正文完整、无明显溢出。读取状态明确只核对，未删除、恢复或自动合并。详情见[P2E](VALIDATION-SMS-CANDIDATE-REVIEW-P2E.md)。未在真实页面验收跨客户端删除/恢复。

## 构建、交付与私有证据入口

固定e0f843b完整CI [39954381214](https://github.com/Viper-Boss/RustSimHub/actions/runs/39954381214)通过；251项Rust测试、八个生产ARM程序、七组网页及同源真实ARM Native26项通过。正常签名暂存、选中、健康核验和精确manifest确认已完成，构建与升级失败报告保留。

- 冷启动前一致性备份：`.private/sms-review-coldboot-backup-20261023-first.json`。
- Wi‑Fi先行/实际启动/数据：`.private/sms-review-coldboot-health-20261023-first.json`。
- NAS既有USB配置恢复：`.private/sms-review-nas-usb-activation-20261023-first.json`。
- 单次只读通话观测/最终日志：`.private/sms-review-call-20261023-first.json`、`.private/sms-review-call-inspection-20261023-first.json`。
- 用户正式页面截图验收：`.private/sms-review-browser-acceptance-20261023-first.json`。
- 临时信任及移除：`.private/sms-review-browser-trust-20261023-first.json`。
- 最终独立汇总：`.private/sms-review-coldboot-acceptance-20261023-first.json`，passed=true。

只读运行中的两份相同数据库副本用于诊断，未把它们称为一致性备份。原始日志、SIM信息、正文和截图不提交GitHub。后续继续双向/来电接听、正式短信跨客户端管理、候选期间新数据回退、完整OTA/恢复和长期测试；关机问题继续搁置。
