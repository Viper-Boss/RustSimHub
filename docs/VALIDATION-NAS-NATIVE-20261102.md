# NAS 只读客户端原生集成记录（2026-11-02）

本轮完成主线负责的构建与实际 NAS 运行准备，不接管成员 C 的功能实现。未授权请求在真实设备上被拒绝；成功授权读取仍未验收。

## 来源与构建

- 组件固定来源：`a5df2c15eb377659d7b7a48d827eea6cbdeeae96`。归档只包含 `tools/nas-adapter`、兼容黄金向量和 LICENSE，不称完整仓库快照。
- 组件归档 SHA-256：`9e73d8e91d67d1949760a0654daf8f3d74ecc288ad38e9c61ecfe657ac71d9ea`。
- 使用 Go 官方下载元数据核验的 Linux SDK，在本机 WSL 普通用户独立目录运行；未在 NAS 或棒子安装编译器，不修改全局 PATH。SDK 来源：[Go 官方下载](https://go.dev/dl/?mode=json)。
- `GO111MODULE=off go test -count=1 -json ./tools/nas-adapter`：27 个测试用例通过，无失败/跳过；`go vet ./tools/nas-adapter` 通过。它们是本机 Linux/回环 TLS 测试，不当作 NAS 实测。
- `CGO_ENABLED=0 GOOS=linux GOARCH=arm64 go build -trimpath -buildvcs=false` 成功，核验 ELF64/AArch64 和实际构建元数据。
- ARM64 客户端 SHA-256：`181d2eacb65bf4eee5bf2010e743a129b6383e23901f245deff735c2d35aa138`，9,185,979 字节。现有 Go/AGPL 来源保持，没有改称 Rust 实现。

## 实际 NAS 与设备结果

1. 核对 NAS 为 AArch64/64 位、普通用户 UID 1000，在已有数据目录中新建独立 0700 测试目录。未改 NAS 系统配置、容器或自启动服务。
2. 通过正式 v2 Identity 接口、准确 Origin 和受信任 HTTPS 读取设备公钥。没有读取设备身份私钥或重置所有者。
3. 当前配置未登记 USB HTTPS origin，因此使用已登记的局域网 IP origin。没有放宽 Host/Origin 或绕过 TLS 检查。
4. 客户端上传后回读逐字节一致，二进制 0500，独立 NAS seed、配置和证书均 0600，父目录 0700；独立 seed 从未获批，不复制 Owner 或设备私钥。
5. 在 NAS 原生执行一次 `session`：未批准 NAS key/binding 返回 `unauthorized (HTTP 401)`，退出 1，stdout 0 字节。
6. 在 NAS 原生执行一次 `read`，使用随机无效 bearer：同样返回 `unauthorized (HTTP 401)`，退出 1，stdout 0 字节。没有输出业务记录，没有自动重试。
7. 前后九个应用/网络/灯控单元 active，转发 PID 0，普通蜂窝数据未连接，boot ID 相同；未重启、停组、切网或刷分区。

## 失败记录与待做项

- 首次只读 Identity 预检缺 Origin，得到 403，记录保留；补齐正式协议头后成功。
- 原生第一轮在 USB origin 预检时退出，尚未向 NAS 上传。
- 第二轮在 SFTP 独占打开的写模式错误处停止，留下隔离测试目录；未运行客户端。第三轮改用独占写模式，在新目录完成，无覆盖旧现场。
- 原始日志、地址、独立测试凭据与报告仅在私有目录。权威通过报告为 `nas-readonly-build-20261102-first.json`、`nas-readonly-native-20261102-third.json`；前两次失败不改写成成功。
- 尚需现存 Owner 通过正式登录批准独立 NAS principal、绑定 legacy 映射，再验证真实 status/短信/通话只读读取、撤销与代次失效。未代替 Owner 登录、未手工修改授权数据库；人工步骤放最后。
- 本客户端不是旧 NasSimHub 完整界面、protocol 1.5 全量载荷、短信写入或 PCM 桥。不能将两次拒绝路径通过记作 NAS 功能完整验收。

原 P0–P7 验收 **81.5%**、行动台账 **20/169** 保持。主线代码约 90% 仅为开发粗估，尚未逐项加权核算，不用于验收计分。
