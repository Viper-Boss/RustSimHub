# A13～16：固定SDK fork的兼容补丁候选与实际验证

本轮从main `97f9c52fa832591a5e260d3dc4181847a8b3dfe3` 开始，账号wnfhsj88。
A26已由负责人合并PR144，A27隔离评估另交PR153；本包不重交其实现，不改变
产品引擎的默认OFF/unavailable、Root权限、凭据、配置或主线网络状态。

## 固定来源，不假称官方版本一致

采用任务书指定的nknguard `c32952d7586676fec10f71a30b22969cb24881f3` 中完整
`third_party/nkn-sdk-go` / `third_party/ncp-go`，共31个文件。每个文件都有实际
Git blob、SHA256和字节数，见 `tools/nkn-build/sdk-evaluation/source-manifest.json`。
复核了SDK的RemoteSubClients和NCP FullMesh原修改说明、原Apache通知；Go模块
声明v1.4.8/v1.0.5不等于这些本地修改的内容证明，不用官方sum冒充fork摘要。

实现源码只下载/复制到忽略的work阶段，不提交vendor/算法/二进制/缓存。候选仅
提交186行针对网络生命周期/日志/响应的diff及前后摘要，不改NKN密码算法或协议。
原作者、许可和全部非修改源保持。具体组合分发许可仍由主线审核，非本文裁决。

## 真实观察及候选行为

| 同一触发条件 | 原固定fork实际观察 | 候选补丁实际检查 |
| --- | --- | --- |
| 首次拨号被拒绝 | NewClient返回nil/error，但留下无返回句柄可关闭的reconnect worker | nil-safe Close取消原生命周期并关闭原worker |
| 客户端固定出口拒绝后读钱包RPC | NewWallet没有继承自定义dialer，实际绕过回调到达本机夹具 | wallet保留原timeout/concurrency/dialer，拒绝在实际socket之前生效 |
| 活跃重连中Close | IsClosed=true后仍继续多次拨号尝试 | 同生命周期context传给RPC/WS、等待timer；worker退出后不再拨号 |
| 畸形JSON响应正文 | 上游logger实际包含完整合成敏感标记 | 删除原正文日志，解析仍失败，不把错误当成功 |
| 2MiB合成合法JSON响应 | 原版实际接受完整响应 | 固定1MiB RPC正文上限，读取max+1后拒绝，不再无界ReadAll |

以上socket正向只到本进程创建的loopback HTTP/WebSocket；其他地址均在DNS/
socket前拒绝。不会连接真实NKN、RPC、STUN或链。五项测试执行同一实际SDK，
显式分原版/补丁模式，原版测试“通过”表示确认了缺口，不能记为原版安全通过。
RPC合成高度42只是夹具值，不是链证据、业务成功或NKN Ready。

补丁还使Reconnect发送与Close关闭channel共用原锁，阻止关闭后迟到的连接被采纳；
不新增外部身份/Root证明格式，不延长授权或自动续租。1MiB是**待审SDK兼容预算**，
与本地2048B帧不同；大订阅列表等兼容性仍需评审，不拿此上限扩大T5业务消息。

修改不保证SDK所有cache janitor同步消失、所有日志/错误脱敏或所有密钥内存擦除。
定制dialer须真实遵守context；WebRTC/STUN路径未启用/未验收。原process/cgroup
所有权、真实退出与Root独立释放仍由A25/主线执行，不凭IsClosed或补丁自测代替。

## 实际构建、依赖和剩余入口

工具默认plan，无下载/运行；显式fetch只取固定官方公开GitHub源，显式run在新
work阶段逐文件验证，原版race五场景及四个原作者内存路由测试后，再干净复制并
以fuzz=0检查/应用补丁，核对两个前后摘要及其余29文件不变。补丁版同样运行
race五场景/四原路由回归、vet，六项Python来源/补丁篡改检查也实际执行。
原作者4测试保留，不能算本人新增。依赖准备后所有运行GOPROXY/GOSUMDB=off、
mod=readonly及go mod verify，不静默联网升级或改registry源码。

真实Go1.27.3编译native/ARM64两个含SDK的**测试probe**并核对ELF，最终native
probe再次真实执行；ARM只编译。它们不是已连接SDK的stdio引擎，不是独立空缓存
可复现制品、发布包或签名安装。真实生产启动/身份/固定出口、A22授权状态采集、
统计交易/确认等仍未接入，原Go前端保持未链接SDK且明确unavailable。

37个实际本机测试编译模块的sum/许可文本摘要及OSV查询在
`docs/dependencies/nkn-sdk-compatibility-candidate.json`。这是选中包闭包，不冒充
未选整个历史模块图。OSV有6个模块版本匹配（websocket、dtls、stun、x/crypto、
x/net、x/sys），保留GO/GHSA标识；重复标识不算独立漏洞数量，未完成函数可达性
审查，不据模块匹配断言各API均可利用。没有批量升级或称依赖安全审查通过。

主线需先审真实fork+此补丁/1MiB取舍与依赖处置，再批准最小SDK启用、独立身份
和固定Root出口。获审后将SDK放在精确受管子进程中接现有前端/监督端，重做真实
完整引擎两架构独立构建及端到端状态/失败矩阵；A26/27应用安全和D的真正Core桥
仍是单独门禁。不读取生产秘密，不因候选补丁把全程可用或整项完成写入台账。

新工具/测试AGPL-3.0-only，OpenAI Codex辅助；原SDK/NCP通知、NKNGuard修改与
固定来源不改写。完整命令/精确受测HEAD/日志摘要见本轮成员交付报告。
