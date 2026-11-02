# 应用71主线集成安装（2026-11-02）

## 固定来源和范围

本轮安装已合并主线 `d78dc2f3b88bad78cf590d24baf897e3d8fc3744`，含NAS当前绑定查询、条件保存/撤销、管理页和本机回执导出/精确清理。源码归档SHA-256 `8fb960e06ff84cf57d6a67c140727a739737baba577d1bad732dcece9161472e`，实际清单1285文件。

正式本机WSL构建完成12个ARM主程序，完整Rust1356通过/0失败/6项原有专用隔离忽略；同源原生音频ARM QEMU34项、固定归档27组网页测试通过。复用已核验缓存，不宣称空缓存构建；没有在棒子上编译。既有开发签名与设备当前信任一致，未新建密钥或信任根，不称正式生产发行。

应用格式2，固定6载荷，schema6、epoch71，合计22,867,057字节；实际清单SHA-256 `1ad8a5222c3ad99b228726fac81c693fe2c3265d2d784dacd7bd58ade63d3e75`。本机OpenSSL/真实ARM更新器验签、6项错误密钥/签名域/损坏/旧schema拒绝测试通过。

此前单独冻结f598d792的另一份71候选未安装、仍保留为历史材料；本次安装的是d78dc2f3这一份，不借用旧载荷或来源。现在floor71，旧的同epoch候选不能用来再次暂存。

## 为什么保留Root程序

已核对安装基线825a5016至本次主线的Cargo.toml/Cargo.lock、setup-ap、core-launcher及其固定Root契约材料，没有功能/依赖变化；两角色不依赖此次新增NAS wire命令。实际Root/Core安装契约一致，8目标中6个静态材料逐字节相同。新构建的全源码来源戳导致二进制hash变化，不代表增加Root能力。

本轮只更新应用组；正式setup-ap、core-launcher、update三角色字节保持。新更新器仅放独立固定维护目录用于有限准入，未替换正式Root路径。候选准备报告中的root_helpers_need_refresh仅为hash差异，不冒称Root功能缺失或已刷新。

## 实际执行

1. 只读确认confirmed70/floor70、无候选、七服务active、呼叫空闲、普通蜂窝数据断开、转发PID0、配网无在途记录、恢复预算未使用；设备公开信任与既有开发签名匹配。
2. 在新的私有维护目录上传固定应用包/工具，逐文件回读；正式和候选更新器均在真实设备验签epoch71及精确清单。上传阶段没有stage/choose/confirm。
3. 注册有界恢复保护，暂停应用组并持Core实际writer锁，归档选定业务/配置及更新ledger；下载回读SHA，SQLite integrity_check通过、唯一Owner保持。该备份为业务树一致性备份，不是全盘镜像、网络计量快照或物理恢复演练；原历史完整快照不删除。备份阶段完成后先恢复70。
4. 再次暂停应用组，在正式版本目录挂载视图内执行admit-stage。信任/已安装角色/实际systemd配置/数据写锁检查通过，Root与当前/候选/回退三角色比较兼容；仅暂存71，没有修改Root角色文件。报告installed_trust_verified、slot_identity_verified、effective_systemd_verified为true；authorization_verified仍为false，按原样保留，不冒称已实现网络发起维护授权。
5. 单次启动完整应用组，正式app-select选择实际71；保留恢复保护。核对七服务、实际音频路由7/7、可信HTTPS六资源与冻结源码逐字节一致、原身份/证书/设置及受保护业务表保持。旧127条短信逐行完整保留，前后均127；发送/通话操作和Unknown记录保持。
6. 健康通过后才用正式更新器确认：confirmed71/floor71、candidate=null、attempted=false。停止恢复timer；独立只读复核全部6载荷与3个运行进程实际exe摘要、同一boot、Root角色原hash、七服务active、转发PID0、普通数据断开、PCM closed、恢复timer/service inactive且预算未使用。

本轮没有重启棒子/NAS、切Wi-Fi、启普通APN、拨号、短信/推送、刷分区/boot/基带/NV或关机驱动试验。没有创建测试Owner、撤销授权、回滚业务数据库或改现有主线硬件配置。

## 恢复夹具和保留的失败

有限guard只接受本次工具摘要、原confirmed和本候选；恢复预算已用或链接标记、陌生Root/候选/confirmed、损坏或链接工具、坏状态均拒绝再启动。尚未尝试的候选只执行一次choose后交正式app-select回到原组；已尝试候选直接由正式选择流程回选。新组已确认则不动作，不手工改ledger/floor或选择链接。

本机12场景隔离夹具通过；这是脚本/模拟systemctl验证，本轮真实业务全程成功，**没有实际故障回退或冷启动验收**。首次夹具构造损坏程序时试图写0555文件而失败，改为仅在夹具构造阶段临时可写后重新置0555；失败目录/报告保留，未放宽产品检查。日志汇总首轮误把Linux路径交给Windows Path，仅本机日志读取失败，未触发设备动作；改用报告中的原Linux路径读取，不重跑构建/上传。

## 接手和未验收项

主工作目录私有记录：official-builder/main-native的20261102-d78dc2f3报告、app71-build-summary/web-frozen、mainline-candidate-device-preflight-20261102-integration-first、candidate71-d78dc2f3/verification integration、app71-codex的upload/backup/admit/start/confirm、app71-final-readonly及guard-fixtures第二份通过报告。原始日志、设备标识、真实数据、签名材料及恢复脚本不提交。

下一步继续主线自动集成/真实故障矩阵；NAS通话/音频客户端、NKN、安全会话等离线实现仍按C/B/D分工，不重复接管。手机热点、浏览器原生下载与视觉批准、听音/物理冷启动等人工验收最后；NAS真实Owner批准/读取仍需合法Owner会话，不能用设备钥匙或测试Owner绕过。

本次交付和基本健康不等于完整UI、NAS、电话、OTA或恢复全部验收。原P0–P7 **81.5%**、行动台账 **20/169**、加权10.6%保持；主线代码约90%仍为未加权粗估。
