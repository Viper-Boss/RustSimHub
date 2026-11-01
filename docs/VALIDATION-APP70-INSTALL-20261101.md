# 应用70实际安装与收尾（2026-11-01）

本记录是主线维护安装结果；不替代手机热点配网、冷启动、BLE或远程音频的实际验收。

## 来源与实际结果

- 应用源码固定 `825a5016e3aa0524190af12139bf908bdd6378e8`；候选清单 SHA-256 `9b27d719a9f4d2749b8d899c1e85a5769982fdecfdd1f4cffd7e59a19fafbba4`，epoch70、schema6。使用设备已有开发信任，不是正式发行签名。
- 构建1254文件逐个核对、1353通过/0失败/6项原有专用隔离忽略；12个正式ARM程序加同源音频，共13个ARM程序，音频QEMU34项通过。复用已核验缓存，不宣称空缓存可重复构建。精确源码CI41296075720两项成功。
- 六文件应用组及八目标Root材料上传后逐字节回读；设备上新旧两个真实更新器均验签通过。
- 先备份旧AP助手和launcher，再注册有限恢复定时器；停止业务后持实际writer锁生成一致备份，取回本机核对归档路径、SHA-256及SQLite integrity_check，没有用运行时热拷贝代替一致备份。
- 只替换签名材料内两个有字节差异的固定Root二进制：setup-ap和core-launcher。其余六个固定目标保持，原安装更新器不替换。
- 实际受限admit-stage通过信任、安装槽、有效systemd、数据及回退兼容检查后暂存70。一次启动，经健康检查后confirm；最终floor70、confirmed70、candidate=null、attempted=false。
- 七个业务服务active，辅助网络/灯控/载波语音/音频服务正常；只读音频routes_on=7/7，PCM关闭。可信HTTPS实际五个网页资源与冻结源码逐字节一致。
- 原管理员、所有原收件记录和其他受保护表、身份及设置文件保持。普通蜂窝数据断开，转发MainPID为0。最终恢复定时器与服务inactive，未消耗自动业务恢复标记。
- 没有拨号、发短信/推送、切换Wi-Fi、刷分区、修改基带/NV/校准，也没有调用关机驱动。

## 状态分区空间处理

准入前状态分区可用约4MiB。仅迁移五份app58～62历史备份，当前业务、当前应用及最新恢复材料不动。先建立根分区归档及本机独立副本，核对68条目（48文件、20目录）的内容、权限、属主、mtime及ACL/xattr；从根分区恢复副本再次归档回读一致后，才逐个移除原历史路径。根分区与本机都保留归档及恢复映射，可用空间提升到约22MiB。没有扩大分区或删除历史证据。具体路径和原始数据仅放私有维护记录。

## 恢复与验收边界

- 恢复脚本只接受已知69基线和本候选70。未知候选、陌生Root字节、损坏备份均拒绝。未确认候选通过实际更新器选择/回选，不直接改ledger；确认70后不回退。
- 七组本机隔离夹具覆盖：确认70不动作、无候选恢复、未尝试候选单次选择、已尝试候选恢复，以及未知候选/陌生Root/坏备份拒绝。夹具替身不代表设备实际故障回退通过。
- 本次安装成功，未触发真实自动回退，也未做手机配网或冷启动。首次WSL环境失败和本地模块名错误证据保留，修正验证后才上传，未因此修改设备。
- 私有证据：app70-codex-{upload,backup,admit,start,confirm}-20261101-first.json、app70-final-readonly-20261101-first.json、app70-guard-fixtures-20261101-first.json，以及旧备份迁移双回读报告。脚本、密钥、归档及原始日志不提交。

## PR收敛

本次读取的开放PR共11个，全是草稿。遵守用户“草稿不合并”，不转ready、不合入草稿、不伪造审核通过。后续变化以GitHub为准。

| PR | 状态 | 范围 |
| --- | --- | --- |
| [#52](https://github.com/Viper-Boss/RustSimHub/pull/52) | 草稿，保留 | docs(ble): propose Core recovery authorization before schema changes (D-21a) |
| [#53](https://github.com/Viper-Boss/RustSimHub/pull/53) | 草稿，保留 | feat(ble): inspect proposed service materials without installing roles (D-24a) |
| [#56](https://github.com/Viper-Boss/RustSimHub/pull/56) | 草稿，保留 | docs(remote): propose bounded TURN/media integration for D-25 |
| [#69](https://github.com/Viper-Boss/RustSimHub/pull/69) | 草稿，保留 | feat(setup): add bounded AP rescan with durable admission and recovery receipts |
| [#139](https://github.com/Viper-Boss/RustSimHub/pull/139) | 草稿，保留 | feat(recovery): record atomic trial progress and terminal evidence (D23) |
| [#140](https://github.com/Viper-Boss/RustSimHub/pull/140) | 草稿，保留 | feat(core): bind recovery parameters to the authenticated application body (D23) |
| [#141](https://github.com/Viper-Boss/RustSimHub/pull/141) | 草稿，保留 | feat(core): connect recovery to the original bounded Wi-Fi actor (D23) |
| [#142](https://github.com/Viper-Boss/RustSimHub/pull/142) | 草稿，保留 | feat(core): expose native recovery evidence as frozen T5 results (D23) |
| [#145](https://github.com/Viper-Boss/RustSimHub/pull/145) | 草稿，保留 | feat(core): add authenticated recovery status and strict object bodies (D23/D26) |
| [#154](https://github.com/Viper-Boss/RustSimHub/pull/154) | 草稿，保留 | feat(core): dispatch accepted recovery channels with fenced A replies (D22/D26) |
| [#162](https://github.com/Viper-Boss/RustSimHub/pull/162) | 草稿，保留 | feat(ble): add bounded opt-in service and kernel-checked Core port (D24) |

## 下一步

1. 人工安排最后：手机验证70版热点刷新、错密码回执、正确密码试连/返回热点/确认保存；不重复结果不明的请求。
2. 继续无需真人的主线整装及恢复证据，成员安全会话、BLE/远程媒体草稿依原分工交付。
3. 新正式PR就绪后继续收拢；草稿及外部前置不以本次安装冒称完成。

原P0–P7实际验收估算仍 **81.5%**；完整行动台账 **20/169**。本次是安装收尾，不因新测试或PR数量增加功能验收分。
