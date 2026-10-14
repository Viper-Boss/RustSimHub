# 已导入历史的授权查询

历史 archive 与当前身份、SIM、短信操作和呼叫操作数据库分开。导入不会注册旧用户、恢复旧权限或重发消息。查询结果均标为 archived，历史状态只作展示。

原始归档保留删除证据，公开查询对已删除短信只返回 tombstone 和删除时间，不返回正文、号码或线程内容。

## 启用

历史与实时只读观察：

```text
rustsimhub-core --serve-history SOCKET ALLOWED_UID STATE_DIR IDENTITY_DIR ARCHIVE_DIR ACCESS_DIR
```

历史与完整短信/电话/媒体授权服务共用一个 Core：

```text
rustsimhub-core --serve-media-history SOCKET ALLOWED_UID STATE_DIR IDENTITY_DIR MEDIA_SOCKET MEDIA_UID ARCHIVE_DIR ACCESS_DIR
```

旧模式继续拒绝全部历史命令。ARCHIVE_DIR 来自离线导入；ACCESS_DIR 为另一个空私有目录，两个目录须规范绝对路径、当前用户所有、0700；文件禁止链接及其他用户读取。运行时对 archive 持有共享锁，必须停止查询服务后才能再次导入。

授权库绑定设备公钥、当前业务数据库的 durable epoch、整个 archive 的 SHA256。正常重启保留授权；换身份、恢复成另一业务 epoch 或更换归档时启动失败，不能自动继承旧授权。由 owner 核验新来源并显式选择新 ACCESS_DIR、重新授权。运行时 archive inode 元数据变化会停止查询，辅助授权/个人标记行必须通过摘要校验。

## v2 命令

使用现有 `--request-auth SOCKET` 的私有 JSON stdin，或 Edge 严格 HTTPS `/api/v2`。所有请求需当前设备签名登录产生的 token；公钥和身份以 Core 现存 principal 为准。

| op | 额外字段 | 权限与结果 |
|---|---|---|
| history_sources | 无 | status.read；仅列出当前可访问的来源和类型 |
| history_page | source、entity、after（初页 null）、limit | sms.read 或 calls.read，加该来源的历史授权；最多16条摘要和游标 |
| history_detail | source、entity、record_id | 相同权限、来源、用户筛选；返回单条公开业务字段 |
| grant_history | public_key、expected_generation、source、legacy_user、entities | 5分钟内重新登录的 owner、clients.manage；目标必须是当前已批准的非 owner principal |
| revoke_history | public_key、source | 同样近期 owner 权限；撤销指定来源的授权 |

entity 为 sms/calls/contacts。联系人以 sms.read 为基础权限，且还须显式授权 contacts 类型。source 为导入时选定的64位小写十六进制 namespace。record_id 为归档独立序号；不是可访问其他来源的通行证。个人字段读取根据旧系统 user_lines/contact.owner_user_id 筛选，用户 ID 不由查询者临时声明。

普通 client 必须指定已存在的 legacy_user。NAS 可指定某个旧用户；只有 owner 显式以 legacy_user=null 授权 NAS，才允许读取此来源全部用户。NAS 身份或原有短信权限本身不开放历史。授权最多128条，类型和公钥 generation 均绑定；主权限撤销、缩减或重新批准为新 generation 会阻止旧授权。

分页游标绑定 archive epoch、实际当前 audience、来源和类型；每次独立重新鉴权，不能通过修改游标或猜 record_id 访问其他用户、来源或类型。来源隔离包含联系人号码。结果不返回 IMSI/ICCID、内部 endpoint、旧用户 ID、私有线路 ID。详情最大48KiB；多号码联系人最多16个，拒绝超限。

按用户授权时，短信/通话返回该用户保存的 read/favorite/marked_unread 状态。共享线路不同用户的标记保持独立；通话不返回全局 read_at 作为个人已读状态。旧库没有对应个人状态时 user_state=null。Owner/全来源 NAS 不选择某人的个人标记。

## 调试网页

现有网页在 health.history_read_api=true 时提供来源/类型选择、分页和详情。历史页面只读，内容使用 textContent；退出清空记录和游标，读取拒绝后清空本页。owner 的历史授权管理目前通过已认证的 API/CLI 完成。真实浏览器显示、IndexedDB 及刷新恢复必须另外验收，不能以 JS 语法检查代替。

当前新增测试覆盖共享/私有线路、联系人归属、每用户标记、当前签名授权与 generation、撤销、游标和来源隔离、archive 改动拒绝、正常重启及身份/业务 realm/归档变化拒绝。ARM 和实际 Core/HTTPS 验收结果另存 VALIDATION 文档；本文接口说明不代表这些实测已完成。
