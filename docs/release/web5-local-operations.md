# WEB5 本机初装与维护手册

本文说明真实入口和前置，不自动执行设备操作。合同基线是已合221（main
`7535535bc5a95c42ff1ea70743b97be561f2049a`）和待接纳215固定
`610d7fa63b1a0f129a3f146c0595171868f8e059`；首次热点新增接线在PR224。
旧74包不是这些组合的制品或同源证明。整体81.5%和硬件未验收项保持原样。

## 初装与连接

初装前先由本机负责人确认可读整机备份、恢复入口、确切设备及分区目标和签名信任。
原221 `tools/system/build-generic-state.py` 的真实离线入口是：

```sh
python3 tools/system/build-generic-state.py \
 --maintenance-public-key PUBLIC_KEY --expected-public-sha256 PUBLIC_KEY_SHA256 --out NEW_OUTPUT
```

该参数是**本机独立选择的公钥**。包含维护信任的 NSHSTATE 只适合本人自用交付，
不作为公开通用发行；私钥绝不能入镜像。镜像/boot/modules/DTB/固件提取与签名发行门禁
须分别核验；本文不提供猜测设备地址、分区、刷机参数或自动刷写。

原 generic 首启使用本机生成的稳定USB identity和设备名，调用原
`/usr/local/libexec/rustsimhub-install-initialize`。该 CLI 真实参数是
`MAINTENANCE_PUBLIC_KEY LISTEN_ADDR SERVER_NAMES NEW_PRIVATE_HANDOFF_DIR [--media-lan]`，
只有新安装、业务停止、原Root权限/受信应用/持久状态均合格时可调用；重复初始化不是恢复。
Generic wrapper 已提供板级USB默认值 `10.55.0.2:8443`，主机DHCP单租约 `10.55.0.1/29`，
不提供网关/DNS；这是源码固定值，不能据此推断任何现有设备身份。首次 HTTPS 信任用
本机私有 handoff 核对，Owner code/浏览器私钥不公开。

USB维护先可用；按既有页面在原 Owner/权限下配置 Wi-Fi。PR224首次AP只在新设备
身份就绪和原有限 Core/AP事务允许时开放；status prepared不等于 Ready/认领成功。
若采用新模块，Root本机可查询 `/usr/local/libexec/rustsimhub-firstboot-ap --inspect`。
通用设备不得复制旧设备TLS私钥、setup code或Owner。

已有 CLI `simhubctl wifi-trial SOCKET SESSION_FILE UNIQUE_IDEMPOTENCY_KEY` 只试用
**已保存Wi-Fi**，要求 network.manage 和显式 network-trials 服务模式，并始终恢复；
它不是首次保存密码接口。新配置使用原受限设置页面/原AP trial/save/confirm/cancel流程。
切换前保留USB维护路径；USB/Wi-Fi失败不启用普通APN，也不关闭语音需要的WWAN/IMS。

## 备份与恢复

业务备份复用原 Store/outbox 一致性接口，不能复制运行中的 SQLite 主文件冒充联合备份。
身份密文、业务库、outbox、Owner/客户端密钥、TLS和整机镜像范围应分别列清。
现有离线整机备份核验入口：

```sh
python3 tools/recovery/verify-offline-backup.py --image OWN_BACKUP_IMAGE \
 --expected-sha256 EXPECTED_IMAGE_SHA256 --expected-gpt-sha256 EXPECTED_GPT_SHA256 \
 --output NEW_PRIVATE_REPORT
python3 tools/recovery/plan-system-restore.py --image OWN_BACKUP_IMAGE \
 --capture-report OWN_CAPTURE_REPORT --offline-report OWN_OFFLINE_REPORT \
 --expected-gpt-sha256 EXPECTED_GPT_SHA256 --output NEW_PRIVATE_PLAN
```

这些读取已有本机私有材料并写新报告/计划，不证明跨库同一时点或允许刷机。实际恢复
执行器是 `tools/recovery/restore-system.py`，默认 action=validate；restore 要求完整
受信计划/摘要、已核验备份、确切目标、独立确认和原恢复入口。预检工具不调用它。
未知短信/电话/outbox不可改写为待执行；恢复域更新后按原Auth语义重新授权。

## 应用升级、回退和健康

以下是215提供的**本机Root有副作用的维护入口清单**，不是本预检的子命令，也不在
本次云端执行。应用和有限 Root材料分别进入固定
`/var/lib/rustsimhub/rustsimhub-v1/system/app-inbox`、`.../system/product-inbox`，
安装信任固定 `/usr/share/rustsimhub/release.public`，独立角色政策固定
`/etc/rustsimhub/product-roles.json`。包内 key/UID/verified JSON 不可自授权。

`/usr/libexec/rustsimhub/rustsimhub-update` 后跟**一个**动作：

| 动作 | 原行为/前置 |
|---|---|
| `product-prepare` | 实际签名、原确认baseline、模式/schema、NSS、停止事实、维护lease和一致备份均合格才准备 |
| `product-apply` | 同一活守卫持续持有真实业务/outbox writer，顺序迁移及有限 Root写集；安装后默认OFF |
| `product-inspect` | 合格维护上下文重开保留事务；不要把它叫纯只读（可能取得原Store/锁） |
| `product-recover` | 明确新维护调用核对原backup/签名/事务后继续；不会由Unknown自动重试 |
| `product-rollback` | 只按原兼容/归属证明回退本事务目标；不覆盖后来数据、不恢复旧schema/floor |
| `product-activate` | 原显式候选选择/启动入口；候选仍未确认，不能由页面JSON替代 |

纯诊断入口是同一程序的 `inspect-state` / `inspect-runtime`（各自只接受一个动作），
或在停止的维护上下文使用 `inspect-maintenance` 保留原读/锁边界。诊断不授权安装。

`confirm-running-candidate` 是有副作用入口，只有现有有限 v1 原生生产者能通过全部事实
才调用原确认账本：同Core字节、forwarding/schema9/worker4、实际进程/UID/NNP/空能力、
Core真实业务就绪、5秒新鲜度和250ms进程预算。AP、SMTP5/EmailRecovery、不同Core基线
不支持自动确认。不能提高预算、预先推进floor或写“healthy=true”绕过拒绝。

## Unknown 与交付验收

断电、超时、断开、撤权或子进程失败一旦结果不明，保存原幂等键、attempt、journal、
候选与backup；停止业务重复动作。先检查已有事务及实际回执，不能删 journal、重置
lease/回退预算、再发消息/重拨来“试一下”。显式 recover/rollback 要重新取得合格的
本机维护上下文；拒绝说明材料不足，不能凭离线审计报告重建权限。

本机接纳后依次验收：完整同源输入预检 → 隔离完整镜像/真实PID1首启 → USB/HTTPS与
每设备独立身份 → 新AP Ready/认领、确认/取消/撤权、Wi-Fi回退 → 一致备份及可读恢复
→ 候选应用/Root/数据/schema保持 → 符合v1范围的健康确认与原floor账本。
未支持组合保留拒绝；硬件/真实授权/生产签名仍独立验收。
