# NasControlClientV1 / NasAudioPortV1 候选

这是C28独立正常客户端，未修改上级旧NAS只读桥。`New`仅接收另行批准的普通客户端token及明确CA，Core仍决定身份/权限/线路/租约；没有旧NasKey或Owner私钥转换/自动升级路径。调用者不得把配置文件的身份标签当权限证明。

控制方法只有Prepare、Submit、Operation、Cancel、Renew。真实POST `/api/v2`、protocol=2、token在实际AuthCommand内，沿用Core幂等键/operation ID/lease结构；TLS1.3、显式CA+hostname、无代理/重定向/自动重试。**没有在HTTP `/api/v2`上伪接IssueMedia/CheckMediaLease**：Edge已有DTLS peer门禁，媒体授权必须来自现有实际提供者。

Mutation传输失败/取消/非法回执/泛化Core错误保留`Failure.Unknown=true`及原恢复ID，不代表未发送。Core返回operation.state=unknown时原样保留，调用者只查询原ID；适配器没有自动重新提交路径。Prepare的恢复ID是原幂等键，若未取得operation ID仍须使用现有Core/业务核对流程，不能因此换新键重发。读取结果不返回部分内容，拒绝错误protocol/requestID/operationID/line/lease、未知字段、重复JSON、超大/过深JSON和错CA；远端错误正文不写日志。

Linux CLI默认不连网，只输出计划。显式`--run`后才加载私有配置/输入并发出一次请求，供负责人将来集成：

```sh
GO111MODULE=off CGO_ENABLED=0 go build -o /tmp/nas-control ./tools/nas-adapter/cmd/control
/tmp/nas-control
# 以后在已批准环境：/tmp/nas-control --run /private/control.json < /private/command.json
```

配置仅`url/ca_file/token_file`，token文件为64个小写hex字符；文件0600、父目录0700、均归运行用户。每层路径openat/O_NOFOLLOW，拒绝链接/设备/FIFO/硬链接/过大/变化文件。输入只支持上述有限命令；不接受token/scope/任意RPC。成功输出可能含授权通信元数据，应在私有环境保存。本次只运行临时loopback TLS夹具，未加载真实账号或连接NAS/设备。

音频为Linux Rust可选特性`rustsimhub-compat/normal-nas-candidate`，默认关闭。`nas_audio::NasAudioPortV1::open`必须接收实际typed `rustsimhub_media::Permit`、现有非阻塞`Pcm`端口和**同一提供者的可信boot时钟**；不能由NAS JSON或Unix时间构造Permit。仅明确16000Hz/mono/signed16LE/640字节帧，未知8k/stereo等格式直接拒绝，不猜旧格式/静默重采样。复用真实Pump的4帧队列、20ms节奏、60ms龄、序列、过期/撤销/时钟回退及外部I/O后门禁；关闭/Drop关闭端口，不录音/发现硬件。媒体ticket socket只用于授权，**不是PCM传输接口**。实际NAS跨主机音频传输、时钟映射、DTLS peer和硬件接线仍需主线接纳现有提供者。

旧只读桥`legacy-golden.json`已固定来源`ea019485e5d890a96acae357c52b94c746de837f`及原许可；本次保持其签名/只读语义。未取得可审核的固定旧UI及旧PCM协议，所以不宣称整套旧页面/旧音频已兼容，亦不把原Go服务冒充Rust。

验证：`GO111MODULE=off go test -count=1 -v ./tools/nas-adapter/control ./tools/nas-adapter/cmd/control`、`go vet`；Go实际发出的五个命令逐字段匹配[合成向量](request-vectors.json)，Rust候选测试将同一向量交给真实`rustsimhub_wire::parse_auth_request`。本机TLS服务模拟外部Core回执/断线/拒绝，不冒充已运行真实Core授权或设备通话；原Core真实鉴权/租约回归由全仓测试保留。音频测试用真实Authority消费Ticket取得Permit和真实Pump，仅Pcm为外部合成设备，涵盖LE转换、溢出/丢帧、错误格式/帧、撤销、I/O期间过期/回退、关闭。AGPL-3.0-only，新代码由Codex辅助编写，复用仓库现有协议/媒体模块，无新增第三方Go依赖。
