# USB 配网：重启后只读回执契约

实现：`8576d880dea4dd3c97deb74bf7b8c90bfb323d49`。新增普通 USB 业务命令，不涉及热点或 recovery/BLE 协议。

## 调用

工具在 `health.wifi_request_api === true` 时，通过原 Auth protocol2 `/api/v2` 调用 `wifi_request`，字段只有 `token` 和原 `idempotency_key`。示意：

```json
{"protocol":2,"id":"receipt-query","command":{"op":"wifi_request","token":"<当前有效64字符令牌>","idempotency_key":"<原配网意图ID>"}}
```

占位符不是实际令牌。旧服务没有该能力字段/新命令时，明确不支持；不能调用新的 `wifi_begin` 来“查询”。查询不需要 SSID/密码，也不接受 trial_id/path/confirm/接口或其他字段。

重启使旧会话失效；先按原签名流程重新登录。只有**同一当前授权 principal**（ID、公钥、类型、完整scope、代次、active），同一持久业务realm及设备身份，才可读取原新格式回执。其他客户端、Owner接管Client、撤销后重批的新代次、缺权限/NAS均无读取权。磁盘读取后再次检查当前授权；不恢复令牌或扩大权限。

## 返回语义

- 新格式 `.started` 可核验且 `.result` 完整匹配：`known:true`、原 `trial_id`、`terminal_recorded:true`，state仅为 `saved/rolled_back/unknown`。
- 新格式只有开始/确认记录：原trial仍可识别，`state:unknown`、`terminal_recorded:false`。不得推断未执行、未保存或已经回退。
- 无记录或旧schema1没有完整授权/realm绑定：`known:false,state:unknown`；`not_found_is_not_proof:true`。不暴露旧格式中的他人trial，不自动迁移。
- 绑定不符、损坏、异常权限、符号链接、超限等读取失败：沿用原API拒绝，不作为已知失败或不存在的证明。
- 所有成功响应均为 `receipt_only:true,requires_confirmation:false,current_network_verified:false`。历史Saved不证明现在仍连接那个网络，不能覆盖当前网络状态。

原 `wifi_status/confirm/cancel` 仍只控制当前内存 Session。Core重启后，持久回执不是控制权，不能据此确认、取消或恢复旧actor。丢失查询响应可以只读查询原key；不自动重发试连、改key重做、写NM/恢复原profile或删除旧标记。

## 落盘与兼容

新普通试连在任何NM动作及返回ID之前，将原随机trial_id、schema2和完整绑定摘要写入原 `.started`，fsync文件/目录；仍使用原hash(key)路径、配额和create-new防重。`.confirmation/.result`带同一基底。摘要不存SSID、密码或其低熵校验值；不会新增第二套业务状态库。

查询仅读取固定两个受限私有文件，每个最多8192字节、不扫描目录、不写审计。旧schema1文件原样保留并继续阻止重复开始，不将不完整旧证明升级成新授权。升级/回退均不删除任何旧文件：旧版本不支持新查询，但原create-new标记仍阻止同key重发。无数据库迁移、固件部署或设备配置步骤。

本轮验证的是测试进程实际退出/新进程重建Auth，以及实际NM适配模拟中的actor丢失与回执读取。不是整机断电、生产Core服务重启、真实USB工具或设备终验。现场仍归主线。
