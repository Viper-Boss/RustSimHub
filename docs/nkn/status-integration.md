# A-21/A-23/A-24：只读NKN状态展示与接线边界

组件`crates/edge/src/web/nkn-status.js`只消费已冻结的T5 NknStatusProviderV1（t5-offline-v1.json）。constructor/render/reset/重新显示均不请求业务；只有显式点击只读按钮才调用注入readStatus({signal})。默认没有provider，按钮禁止、状态未知。没有引擎start/stop、续订、订阅、测速或网络发现方法，没有任何fetch/存储/凭据接口。

## 实际上游可提供什么

固定只读来源nknguard@c32952d7586676fec10f71a30b22969cb24881f3：pkg/nknclient/status.go的ConnectionStatus/NodeStatus，以及pkg/signaling/nknsignal/health.go的Transport.ConnectionStatus（blob fd5bc10fdc0ebdce6d0a32ce9ca1e35201aa925c）；后者明确只取当前内存/客户端状态，不做网络I/O，不能改为调用Open/NewMultiClient。

| 冻结字段 | 固定上游事实 | 生产provider前置 |
|---|---|---|
| contract/source/generation/sequence/age_ms | 上游ConnectionStatus不直接提供本冻结上下文/单调年龄 | 主线/实际引擎runner建立真实设备/启停上下文、单调代次/序列和证据年龄；不能将HTTP刚到达当新鲜 |
| enabled/service | 上游health.State为checking/connected/closed等消息健康状态，非本地进程/Owner开关的完整证明 | 从实际开关/进程所有权分别映射，不把connected当已启用或已证明running |
| path/dht | NKN消息健康状态不是直连/中继或独立libp2p DHT证明 | 缺真实路径/DHT信息保持unknown，不启动另一个栈补数据 |
| rx_bytes/tx_bytes | 所核对ConnectionStatus未提供这两个累计计数 | null；不能用0、测试payload大小或节点数推算 |
| rtt_ms | LatencyMS来自健康自发消息；失败时重置为0，存在CheckedAt/LastSuccess | 只采纳已确认成功且年龄有效的实际样本；失败不能显示0ms |
| peer_count | Nodes含ClientID/Endpoint/Closed，是SDK子客户端当前节点信息 | 主线确认完整/去重的节点计数语义后才输出；不把成员/安装数或缺少列表当0 |
| last_error | 原LastError可带远端文本/地址 | 映射固定错误枚举；不直接输出Endpoint、标识或错误正文 |

A-22实际引擎适配/授权provider目前未实现：本包交真实解析/显示组件及字段映射盘点，不用固定成功或编造计数填该缺口。上游Go未在本包编译/联网，也不冒称已接Core或真实NKN。

## 严格数据与显示

14个字段必须齐全且无额外键；精确核对source/开关/服务/path/dht/错误枚举、u32代次/序列、非负安全整数或null年龄、有限RTT/节点数。响应最大4096B；私有字段/原始地址/错误额外输入均拒绝并显示有限错误，不进入DOM。rx/tx只接受规范十进制u64字符串或null，校验不超过18446744073709551615，保持字符串显示B，绝不先转Number或把未知补0。

每个开关/进程/DHT/直连/中继字段独立展示：enabled=off与service=running可以明确并排，不能推断无PID。peer_count始终标“连接节点数（不是用户数或安装数）”。source=mock始终显著标“离线模拟”；unavailable或年龄null/负/无效/>5000ms时动态字段均未知，不把旧错误/计数当当前成功。

## 生命周期和宿主责任

单次读取3秒：外部回调之前注册截止，先reject再abort，返回后以单调/墙钟经过时间再次核验，任一回拨/非有限/到期均拒绝。组件保守加入整个本地等待时间，再用两种时钟的最大经过时间更新年龄；休眠/排队不能被简单丢掉，超过5秒只改变显示，不自动读取。

同一上下文generation只能接受严格增加的sequence；较小/相同序号不能替换视图。generation或source改变时锁为未知，不能按数值大小猜新旧或随后恢复旧代次；宿主根据真实设备/引擎/登录上下文显式reset后才允许新代次。reset也使旧请求epoch失效，退出/目标切换后晚回复不覆盖新视图。

隐藏页面清除当前显示/中止本地等待但保留序列证据，返回只恢复手动按钮，不自动读取；dispose移除监听并禁止读取。这里的Abort不等于停止引擎或取消后端设备任务，真实provider负责鉴权、上下文和传输清理。

未来宿主装配示例：

```js
const panel = new NknStatusPanel({document,root,readStatus:authorizedRead});
// 在实际登录/设备/引擎上下文变化时，先使旧授权读取失效，再reset。
panel.reset();
// 页面销毁：
panel.dispose();
```

示例中的authorizedRead未由本包提供，不得改为HTTP里伪造的engine/source或mock后端。主线需注册nkn-status.js静态路由并接纳真实授权provider后才可导入首页；本包没有修改Rust路由/app.js或安装生产后端。

## 真实验证与未验收

```sh
node tools/test-web-nkn-status.mjs
python3 tools/fixtures/nkn-status/browser.py --output /tmp/nkn-status-evidence
```

Node执行真实解析器/组件并逐项对照冻结清单：精确u64、未知/关闭/模拟、错误/多余字段、时钟/3秒边界、年龄排队、序列/代次/迟到/后台/reset/dispose/原生超时abort竞态。fake仅为DOM及读取端口，组件逻辑不替代。浏览器夹具显式为本地合成端口，使用实际生产模块、严格CSP，三尺寸1440/390/320的44px/键盘/超长数字/长中文/无溢出、真实5秒陈旧、无自动读取、旧上下文锁定和显式reset通过；不等于真实引擎/Owner或设备验证。夹具不加入生产资源清单，不提供query/localStorage/隐藏按钮的生产Mock开关。

真实Engine provider、A-22引擎输出映射、Core授权、实际统计与安装数、Android/第二端、ARM/Root/真实路由/网络和无PID退出仍未验收。沿用AGPL-3.0-only，无新依赖/第三方代码复制，OpenAI Codex辅助实现/验证。
