# 正式系统冷启动后的语音与音频初始化

2026-10-23，整体暂保持 **78%**。电话信令恢复，真实声音尚未重新通过验收。

## 已验证的恢复

固定 `0d5664715af2483d82e95df4bf737bbb7275df2d` 的251项Rust、八个ARM程序、同源Native21项和CI39942588830通过。七份Root候选文件经预检查、私有回退备份、受限命名空间检查、原子发布和逐文件读回后安装；没有在运行中重启基带。原临时Go工具已核验后移除。

用户普通拔插前已停止正式业务并制作一致性备份，数据库完整性、唯一Owner及六类保留表摘要通过，随后同步数据。普通拔插后是新的boot；rmtfs确实使用本机只读私有profile，Rust服务将运行时语音偏好设为PS并读回3，Root网络组件使用新二进制。正式签名应用仍是3216745、已确认epoch22，身份、首次认领消费标记及Owner保持。

USB枚举正常，但NAS既有静态连接设置为不自动连接，重新枚举后没有地址。只激活原有固定MAC/无网关/无DNS/never-default连接即恢复严格身份USB SSH；没有修改NAS默认路由、DNS或连接配置。Wi-Fi也已恢复。普通gsm保持断开，没有普通数据APN。

用户再次拨入确认正常回铃；后台真实观察到一个新来电 `ringing-in`，用户挂断后 `terminated`，PCM始终关闭。随后用户只点一次10010测试，实际状态是 `dialing → ringing-out → active → terminated`，累计10010 **42**。这证明来电和拨出信令恢复，不能视为声音通过。

## 本轮音频失败及修正边界

浏览器返回HTTP409并确认挂断。设备媒体日志为 `Unavailable / pcm_opened=false / 0 frames`；DTLS预连接已验证，随后打开PCM前失败。只读固定profile探针匹配，但七项ALSA路由全部关闭；系统原有alsa-restore成功并不代表这七项板级路由已启用。历史 `audio-foundation.md` 已明确平台冷启动初始化尚缺，早期7/7来自此前系统状态，不能替代独立系统启动验收。

新增Native Rust固定命令 `--prepare-routes`：仅Root和UFI003板型，要求四个目标PCM均关闭；先核对既有固定声卡/PCM/control profile，只将七个已知Boolean路由设为开启并全量读回。已开启的路由不写。失败写入也计入不确定变更，按反序恢复自己修改的路由，并明确报告回退是否完整；不会关闭原来已开启的路由。它没有PCM打开、拨号、音频采集、任意控制名/端口或NV写接口。

独立Root平台程序安装在 `/usr/libexec/rustsimhub/rustsimhub-audio-platform`；启动单元只允许 `/dev/snd/controlC0`，没有PCM设备许可，空capabilities、只读系统目录。它在驱动和alsa-restore之后、Core/Media之前执行。平台程序独立于签名应用选择，避免回退到旧应用时因旧Native不认识新命令而破坏整组回退。非Root媒体仍需Core的ACTIVE/权限/租约许可才能打开PCM，HTTP409和鉴权没有放宽。

五项新增合成测试覆盖正常启用、已启用不写、不确定写失败反序回退、读回失败且保留原路由、回退失败报告。首次本机Native26项通过，但Clippy脚本尾部CR导致 `warnings\r` 被识别为未知lint，不能算严格lint通过；第二份LF脚本重新运行。后续ARM构建、受限服务实际验证、安装及浏览器听音仍须分别记录，不能以源码代替验收。

## 保留的数据问题

冷启动后原20条收件箱记录逐字段保持，另产生20条相同内容、指向原记录的 `duplicate_candidate`。差异限于seq/id/line_epoch/duplicate_candidate；这不是20条新收信。尚未解决跨启动重观测的显示/保留策略，不按正文或号码自动合并，避免误删真正重复短信。此前终止呼叫重观测的历史重复问题也仍开放。

私有证据：`carrier-root-delivery-20261023-first.json`、`carrier-coldboot-backup-20261023-first.json`、`carrier-coldboot-health-20261023-third.json`、`carrier-postboot-details-20261023-first.json`、`carrier-nas-usb-activation-20261023-first.json`、`incoming-after-profile-20261023-first.json`、`carrier-outgoing-inspection-20261023-first.json`、`carrier-audio-routes-20261023-second.json`。首份USB超时和第二份短信数量假设失败均保留，第三份健康检查明确记录20条重复候选，未删除或修改失败报告。
