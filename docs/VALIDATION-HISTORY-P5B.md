# 授权历史查询实际验收

2026-10-14。P5 增加1个进展点：原 Go 协议向量/离线合成迁移2点，加本轮正式授权查询与 Core/HTTPS 链路1点，合计3/8；整体63.5点、约64%。真实旧库迁移、NAS在线协议/PCM兼容及浏览器显示仍未完成。

## 版本

源码 `a5e4eb7f0f38581915fb9124160fed1b06f4f584`；精确归档 SHA256 `7f7de34cb6278521e80644ae9d8f606bacc613811a8c5fd5f4a194a6e7859d17`。本机157项测试、全部 lint、ARM64生产构建通过；独立 GitHub CI `38547240598` success。Core production fixtures=false。

实际测试程序在本轮独立 `/var/tmp/rustsimhub-history-api-*` 目录。只有合成历史和独立身份，未读取原始业务数据；系统D-Bus和硬件设备不可访问。TLS监听沿用设备Wi-Fi地址与8448，严格校验证书/hostname/Origin。通过当前设备签名的挑战完成第一个 owner 初始化与三组独立 client/NAS 配对；没有旧用户权限自动继承。

## 实际结果

- 原合成schema加一条B用户私有线路/短信/联系人，共19条；A/B两个namespace共38条归档。公开history API通过同一个完整短信/电话/媒体授权Core运行，不另开共享业务数据库的第二个Core。
- 未启用history的模式拒绝查询；重启进入组合模式后，旧owner token拒绝，重新签名登录通过。
- 原有sms.read/calls.read和NAS身份不打开归档；普通client整库授权、非owner授权、错误generation均拒绝。
- owner显式授权A/B旧用户与NAS整来源后，A见3条短信、B与NAS见4条；来源A授权不能读来源B，其他用户不能复用游标。
- B私有短信和联系人对A拒绝；A联系人号码对B拒绝。共享线路上A/B个人已读/收藏/标记未读状态分别保持。公开详情不含SIM、旧用户和内部线路/endpoint字段。
- 已删除短信仅返回tombstone与删除时间，没有正文或号码；归档中的submitted旧状态没有形成新发送动作。当前Core inbox/calls仍为空。
- archive授权撤销立即拒绝旧游标；当前principal撤销拒绝旧会话；重新批准后generation增加，旧授权和旧generation请求拒绝。重新授权后新查询通过，旧游标仍拒绝。
- 同身份/业务epoch/归档正常重启，重新签名登录后有效授权保留；旧token拒绝，整个archive物理SHA256不变。
- 实际Core/Edge进程UID及CapEff/CapPrm/CapAmb/CapBnd空能力集合核验通过。临时服务停止，旧四服务active、PCM关闭；没有拨号、发送短信或蜂窝数据测试。

报告 `.private/history-api-a5e4eb7-fifth.json` passed=true、cleanup_verified=true。完整构建日志 `.private/wsl-build-a5e4eb7f0f38581915fb9124160fed1b06f4f584.log`，exit=0；产物 `.local/wsl-artifacts-a5e4eb7f0f38581915fb9124160fed1b06f4f584`。

## 保留的失败及边界

首次测试PrivateTmp隐藏了临时程序路径；补上只读bin bind。Windows到设备Wi-Fi的TLS出现超时，实测同时Core只读IPC正常、两个进程正常且资源占用小；该Windows路径仍未重验通过。随后从NAS对同一Wi-Fi地址、8448和证书交叉验证，完整API链路通过。NAS辅助程序曾因http.py命名遮蔽标准库失败，修正后通过。前四轮失败与清理报告保留，没有作为成功证据。

网页已经有历史来源、类型、分页、详情、退出/拒绝后清空显示，JS语法检查通过；本轮没有真实浏览器渲染、IndexedDB或刷新恢复验收。测试用的是独立编写的合成schema，不是真实旧NAS库。本轮不声称全面兼容旧NAS在线协议、双向音频或已刷完整固件。
