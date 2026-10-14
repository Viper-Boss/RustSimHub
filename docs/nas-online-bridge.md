# NAS 在线接入桥

这个入口保留旧 protocol 1.5 的请求签名字节，但读取数据使用明确声明的 RustSimHub data_format=2；旧 NAS 需要最小适配器。没有伪造旧 SIM 标识、呼叫时间或旧 JSON 对象来冒充完整 protocol 1.5 Agent。旧配对、写操作、更新、PQ、KCP、信任策略和 PCM 尚不支持；启用兼容时旧 `/v1/*` 未支持路径返回501，不能默认为普通设备离线或再次夺取 owner。

## 固件启用

默认不启用。Core 的兼容目录由运行用户拥有、700、规范绝对路径，SQLite及锁文件600，禁止链接；只有明确选择下面模式才开放 Core 命令：

```text
--serve-media-compat SOCKET UID STATE IDENTITY MEDIA_SOCKET MEDIA_UID COMPAT_DIR
--serve-media-all SOCKET UID STATE IDENTITY MEDIA_SOCKET MEDIA_UID ARCHIVE_DIR ACCESS_DIR COMPAT_DIR
```

Edge 私有配置加 `"legacy_compat":true`。继续使用严格HTTPS，不添加明文监听，不绑定蜂窝公网，不激活普通APN。

先通过现有v2配对将独立NAS公钥批准为kind=nas；设备身份钥匙不复制到NAS。5分钟内重新登录的owner调用 `bind_legacy`，明确 core_id、NAS public_key、expected_generation、scopes（status.read/sms.read/calls.read的非空子集）。返回 binding_epoch；每次绑定更新epoch并清理旧NAS会话。普通客户端与owner本人不能被当作NAS。`revoke_legacy` 撤销指定映射。

## 签名与会话

`POST /v1/session` 原始正文只含 `core_id`，原签名仍为 method/path/原timestamp/原nonce/Base64(SHA256(body))。头部为原 X-NSH-Core-Id、Timestamp、Nonce、Signature、Device-Id，另要求：

- `X-RSH-Legacy-Compat: 1`
- `X-RSH-Request-Id`，用于端到端响应关联
- `X-RSH-Binding-Epoch`，owner批准返回的64位小写hex
- `X-RSH-Device-Proof`，NAS同一个Ed25519钥匙的Base64签名

额外证明的精确字节为 `RustSimHub.LegacyDeviceProof.v1\0` + 32字节设备公钥 + 32字节binding_epoch解码值 + 原协议签名字节。它阻止替换未被旧签名保护的Device-Id头、跨设备重放和跨映射重新绑定重放。缺额外证明返回device_binding_required；PQ声明明确拒绝，不能悄悄降级。

本地UTC系统时钟检查RFC3339 timestamp，允许±300秒；远端不提供权威clock。nonce摘要在专用SQLite中持久提交，不提前淘汰仍有效的nonce；时钟低于持久高水位时拒绝签名会话，等待本地时间恢复。重启保留nonce栅栏，清空会话。

每个映射最多4个会话，总计16个，会话15分钟单调时间到期，内存仅存token摘要。token仅在兼容授权器有效，不能用作v2会话、owner、系统管理或发送/通话控制凭据。每次读重新检查现存NAS的kind/active/generation、映射epoch和当前scope。撤销主principal或重新批准新generation使旧映射不能自动继承权限。

## 读取与Go适配器

`POST /compat/v1/read`，Authorization Bearer为兼容会话，正文是 `entity`（status/sms/calls/sms_detail）、after、limit（1—16）、record_id（仅sms_detail）。返回 `adapter_api:1,data_format:2,data:...`，data沿用v2稳定ID、线路代次、游标和可用状态；不声明完整旧Agent响应。没有硬件写操作或历史库权限自动继承。

Edge 对兼容请求使用单独速率预算与2个并发位置；普通客户端继续使用自己的预算。Host必须准确，Origin若存在必须准确，JSON类型/自定义标记必须准确，禁止query、Cookie认证和重定向。租约、会话权限、重放保护由Core负责；Edge没有复制业务鉴权状态。

`tools/nas-adapter/main.go` 是NAS Linux上的最小Go桥客户端，使用TLS1.3、固定设备公钥/绑定epoch/CA、独立NAS seed，禁止代理与重定向。配置与seed需用户所有、私有目录700、文件600，禁止链接。`session PRIVATE_CONFIG` 返回短期会话；`read PRIVATE_CONFIG` 从stdin读取 `{token,query}`，不把token放命令行。无自动网络重试。

当前仅为读取适配器，NAS原应用页面、短信发送和PCM/双向声音的对接仍须另外实现验收。不能将CLI连接或原协议签名向量通过写成整个NAS兼容已完成。实际构建、Go黄金向量与ARM/Core/HTTPS结果另存VALIDATION文档。
