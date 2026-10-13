# HTTPS 与不同 UID 的设备验收

2026-10-13，检查点 `a00bd9b3e374e85a6bc26100e82a3327b8b9687c`。
整体约 44%，按固定权重计：P1 的 HTTPS/证书/进程隔离、P4 的受鉴权网络读取及外部 USB endpoint、P7 的产物归属与失败路径验收。
网页真实浏览器、网络写入、WebRTC 和完整镜像没有完成，未因此计入对应完成点。

## 验证结果

- NAS ARM64：120 项 workspace 测试通过（auth22、core29、ctl1、domain9、edge6、media16、policy1、storage31、wire5），默认 feature check/release 成功。
- [GitHub CI](https://github.com/Viper-Boss/RustSimHub/actions/runs/38417095828) 成功：Rust 1.98 fmt、120 tests、默认 core check、Clippy -D warnings。
- `node tools/test-web-proof.mjs` 通过：实际 WebCrypto 检查 pin、精确字节、purpose、kind、scope、generation、双方公钥、签名篡改和字段注入。
- edge 测试真实 UDS、正确核心响应与错误 id/version/oversized reply、wrong UID/可写父目录拒绝、Host/Origin/content/marker/query/size/role 门禁、证书不覆盖和私有权限。
- network 模拟使用实际私有 D-Bus，唯一 owner 改变拒绝；地址白名单、目录上限/重复/外来路径及去除标识字段通过。
- private read 修正只申请 O_RDONLY，actual inode UID/mode/nlink 校验；0400 可读、缺失文件不会创建、hardlink 拒绝。

## 实机

新建本项目锁定的 rustsimhub-core / rustsimhub-edge 账户及 rustsimhub-ipc 组。没有修改旧服务用户。
全程使用新的随机 fixture、私有 state / identity 和离线 Root owner key；owner key 不交给 edge UID。
core 是独立非 root 的只读 `--serve-call-observing`；HTTP 入口使用另一个 UID。
配置/证书只读挂载，PrivateDevices、NoNewPrivileges、LimitCORE=0；edge 隐藏 system D-Bus，Core DB/identity/owner key 与 PCM 均不可由 edge 用户读取。

`tools/device-edge-smoke.sh` 实机通过：HTTPS 证书及 IP 校验、无信任证书拒绝、明文 HTTP 拒绝、静态资产/CSP、
设备 pin 的离线登录证明、SIM ready、8 条已有短信摘要、无活动通话、只读模式发送拒绝、status-only 客户端短信摘要权限拒绝、
撤销后 NetworkStatus 拒绝、foreign Origin=403、Root 错误 UDS UID 拒绝、退出删除 socket。
读取到了 ethernet、wifi、cellular、loopback 的有限 NetworkStatus，没有读取 Wi-Fi/APN 密码或写网络。

外部 NAS Python 客户端通过 USB 网络验证：ssl.CERT_REQUIRED + check_hostname、仅信任本次已核对证书，
device public key 与可信 SSH 取得的 pin 相同；foreign Origin=403、role 注入=400。
外部协调脚本等待退出时观测超时；之后核实原进程已消失、完成标记和私有日志存在、两单元 inactive；没有因观测超时重启测试或把它误判成业务失败。

一个运行样本：core RSS 7068KiB / 4 threads；edge RSS 3804KiB / 1 thread。
只作为样本，不能声称完整负载/公网/长期性能目标已达成。
单元均已停止；旧 nassimhub-agent、MM、NM、rmtfs 均 active，所有 PCM closed。
无新短信、无新拨号、无蜂窝数据激活、无 mixer/分区/NV 写入，无旧业务 DB 访问。

## 修正过程

初版源包时间早于旧 release 完成时间，出现测试新、产物旧。通过实际 `network_status`/GetDevices 标记确认，拦住部署，
改为当前解压时间与编译时源包 hash，重新构建后验证 --build-info 和真实 NetworkStatus；旧候选没有被当作成功产物。
首次工具检查发现板上没有 curl，在创建状态/启动服务之前退出；改用既有 OpenSSL，不安装额外系统包。
PrivateTmp 先隐藏路径，InaccessiblePaths 必需项找不到使 namespace 失败；改为对已经隐藏的私有路径使用可缺省前缀，保留 UID/目录和 D-Bus 隔离。
随后 private read 申请 O_RDWR 导致只读挂载拒绝；修正 O_RDONLY 并验证打开的 fd，未通过改成可写挂载绕过。

## 源包与二进制

73 个允许列表源文件匹配该检查点；源包 SHA-256：
`78bfc723f022e8b8d79eac6c817c94d763802473dc3fde7f2fe53ab52342c8a6`。
三个程序 --build-info 均自报同一源包 hash；core 标明 development_fixtures=true，不能用作生产固件。

| 程序 | 字节 | SHA-256 |
|---|---:|---|
| core | 5461520 | 63afe50c40167f02a76cacd4a219680d7148bc4b48f8bf2b0e7bfeab28c88cf6 |
| simhubctl | 857104 | c1dcd623c7d56c773429340ccd09b219e375fd4de17b0153f0d3091a9f722966 |
| edge | 2298864 | 7d73067337ac6792ab6faf1494df8be11befc0d165929809ad30855dbf37a2c6 |

棒子 `/opt/rustsimhub/dev-edge-p1/`，本机 `.local/artifacts/*-edge-p1`；上传/安装前后 hash 一致。
完整构建与原始日志保留 `.private/edge-readonly-final-build.log`、`edge-readonly-provenance-metadata.log`、
`edge-device-smoke-readonly-fixed.log`、`edge-device-external-window2.log`、`edge-nas-external-smoke2.log`、`edge-safe-readback.log`。
设备随机目录指针 `/var/tmp/rustsimhub-edge-smoke-evidence` 是私有证据入口，不是可用于证明进程仍在运行的 liveness 信号。

## 待验收

真实浏览器配对/登录/显示、网页短信与呼叫写流程、首次 owner 向导、正式证书、Wi-Fi 配网/切换/回退、
蜂窝授权与流量预算、relay、Opus/WebRTC、生产服务包/签名更新、断电冷启动及整机恢复。
现有只读网络快照与 USB HTTP 入口不能代替 Wi-Fi 或蜂窝 data 的连接验收。
