# X1U · 关闭期间与重新开启的转发游标

2026-10-27，主线FWD-11。源码、隔离集成及固定提交完整CI已通过；FWD-11完成，不表示已部署。

## 语义与实现

默认只处理新消息。最近登录的当前Owner开启或重新配置时，Core采样真实业务库的已提交事件头；该起点与新规则代次在转发库同一事务落盘。重复同一个请求只返回首次结果，不以重试时的新事件头覆盖首次起点。明确历史补发另归FWD-12，本项没有自动补发入口。

关闭会使未领取任务变为cancelled并清理其请求内容，已领取任务保持unknown；旧代次/旧attempt的迟到结果不能恢复成confirmed或重新入队。暂停期间新短信仍正常进独立业务收件箱，开关路径不写入或删除业务短信；转发捕获不扫描这些事件；重新开启直接采样当时新起点，不靠逐页读取来丢弃旧消息。目标凭据与规则留在原私有转发库，网页开关不需要回显凭据。

本轮发现并修正一个读取顺序问题：`claim_current` 在判断关闭前会先查询业务事件头和线路。现在延迟读取线路元数据，先验证当前Owner、队列绑定、代次与启用状态，再调用业务源；关闭且无显式合成测试租约、或代次已失效时，不调用读取器；明确启动的短期合成测试仍需Owner/线路元数据复核，始终不捕获短信正文。复用原claim实现，没有增加第二次策略解析/队列查询，没有移动实际发送前的Owner/SIM复核，也没有新增后台轮询或进程。

## 真实验证与边界

旧实现的真实Owner IPC测试：在临时SQLite业务库移除合成 `inbox_events` 表，验证事件源确实不可读；关闭的capture返回Disabled，但claim不能返回Disabled，唯一用例实际失败。修正后同一用例通过，证明关闭不会依赖可读取的短信事件源。仍执行Owner权限验证，不把“不读短信源”描述成“不做鉴权”或“零磁盘I/O”。

另一真实Owner IPC用例连续3轮：每轮建立在途与未领取任务→关闭→暂停期间写入65条合成短信（超过两页）→关闭实际转发DB文件锁并重开→重新开启→模拟开启回复丢失后以相同ID核对→捕获并确认唯一的新消息。核对：旧游标在暂停/重开时不前进、开启起点准确、旧queued不复活、每轮旧inflight保留unknown且旧finish拒绝、开启后只有新消息入队，不自动补发暂停期间积压。

队列读取器用例还覆盖关闭/旧代次时回调不执行，以及业务读取失败不产生reservation、不消耗queued任务；后续有效读取仍可领取原任务。首个测试夹具缺少适用的UUID生成器/使用了不可见方法，编译失败记录保留；改为合成固定UUID与公开的真实Owner Binding，不增加依赖或扩大生产接口。

本机Linux隔离源码截面721文件，SHA-256 `7cd011e7eb2408ad94b4c722ce10f9d0c30bda99239c7b92818bd79cf970e9d1`：

| 命令 | 结果 |
|---|---|
| Core/forwarder `cargo test --all-features --locked --offline` | Core149项、forwarder73项全部通过 |
| 同两包 `cargo clippy --all-targets --all-features --locked --offline -- -D warnings` | 通过 |
| `cargo fmt --all -- --check` | 通过 |
| Core/Edge/forwarder `cargo check --no-default-features --locked --offline` | 通过 |

使用现有无登录nobody身份、空能力集、NoNewPrivileges、私有网络/设备/临时目录，只读源码/依赖；均为实际SQLite/Auth/IPC和合成消息，未访问NAS/棒子、未发送真实短信或推送。本项不替代FWD-04的真实PID1进程关闭验证、FWD-18/19冷启动，或FWD-20应用/Root回退验收。

固定源码提交 `adcefbee7f7071e2fba8bb4dcfe18ed66b24dd47` 的 [完整CI 40383795138](https://github.com/Viper-Boss/RustSimHub/actions/runs/40383795138) 已逐任务核对：test和native-audio均success，包含真实PID1/凭据/完整cgroup及Owner主管夹具、生产特性检查和严格lint。此次仅延迟读取顺序与回归，不新增依赖或架构相关实现；FWD-10的全新ARM生产组证据仍归其原固定源码，不冒充本次ARM构建。

下一步推进FWD-12显式历史补发：明确范围、数量、目标及内容模式预览、Owner确认、防重与独立回执；取消不外发，Unknown只核对。实际设备仍应用50，转发OFF，尚未部署本轮源码。
