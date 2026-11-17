# USB 隐藏 Wi-Fi：显式入口与兼容行为

本接口服务 USB 电脑工具，不启用热点或修改热点页面。实现来源为 `72bd90fa9b1adb50ed7eac560b582602c83eca09`，仍待主线审核/合并、工具接线及现场验证。

## 工具调用

先在原已授权会话中读取 `health`。只有 `wifi_hidden_provisioning_api === true` 才可提供隐藏网络入口；字段缺失或 false 表示当前端点不支持或未启用，不能将 `wifi_provisioning_api` 当作隐藏模式支持。已有客户端无需改变，继续使用 `wifi_begin`。

用户明确选择隐藏网络后，通过原 `/api/v2` / Auth protocol 2 发送：

```json
{
  "protocol": 2,
  "id": "local-request-id",
  "command": {
    "op": "wifi_begin_hidden",
    "token": "<原会话的64字符令牌>",
    "idempotency_key": "<本次意图的稳定原ID>",
    "ssid": "<用户输入，1至32 UTF-8字节>",
    "passphrase": "<原WPA2个人网络密码约束>"
  }
}
```

该示例使用占位符，不是可重用身份。新命令没有接口、设备路径、shell、模式、超时或蜂窝参数；未知字段严格拒绝。不能给旧 `wifi_begin` 偷加 `hidden:true`，该字段会被拒绝；旧服务器也会拒绝未知新 op。无支持时明确不可用，不自动退回普通连接或蜂窝。

`wifi_status`、`wifi_confirm`、`wifi_cancel` 继续使用返回的原 `trial_id` 和发起者原会话；无需隐藏专用确认命令或新状态库。当前 Owner / 已批准 Client 的 `network.manage` 门禁保持；NAS、未批准和缺 scope 均拒绝。换调用者、改密码/SSID、或同一 key 切换普通/隐藏模式都不能复用旧回执。断线结果不明只查询原 trial，不新建意图重发。

## NM 行为与边界

- 仅固定 `wlan0` 基础设施/WPA2 配置；试连时 `persist=volatile`、`bind-activation=dbus-client`、autoconnect=false，原确认成功后持久化。
- 普通命令写 `hidden=false`，新命令显式写 true；保存也保留同一模式。精确配置检查验证 UUID、接口、SSID 和 hidden，不能以其他配置的成功代替。
- NM 可省略等于默认值 false 的字段；缺失按 false 解释，错误类型仍拒绝。隐藏意图的标志丢失不能报告 Saved；不能证明清理完成时保留 Unknown。
- [NetworkManager 官方说明](https://networkmanager.dev/docs/api/latest/settings-802-11-wireless.html) 指出 hidden 用于不广播 SSID 的网络，可能需要可识别的定向探测。因此采用显式新命令，不把所有手工输入都改为 hidden=true。
- 这证明有限配置与事务接线，不证明真实隐藏路由器、射频扫描、DHCP、USB 链路或断电重启已经通过。没有新增热点扫描、任意 RequestScan 或任意 NM 接口。

## 留给工具/B/主线

工具实现位于本包排除的 `configurator/` 或外部程序，本轮未修改。工具维护方需消费上述能力字段和新 op、保留原回执，明确展示隐藏网络选择及定向探测含义；B可消费固定产品提交构建，不执行来历不明的测试二进制。现场由主线验证 USB 配对/撤权及真实普通、隐藏路由器，不可把本机 NM 模拟记成现场验收。
