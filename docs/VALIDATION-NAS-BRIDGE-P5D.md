# NAS 只读会话桥实际验收

2026-10-14。Rust Core/Edge 提供可选兼容授权器，NAS 上独立 Go 客户端通过严格 HTTPS 连接棒子。旧签名黄金向量与实时读取桥是分别验收的功能；原 NAS 页面、写操作和 PCM 不包含在本轮结果内。

## 构建与来源

Rust 源码 d20b9afcb39be812da026799138d868e78451644，精确源包 SHA256 a1a894f38571dfe87a669c936f38f62af7d21640af0b3e931060da772745f183。本机 WSL Rust 1.98.0 的163项测试、lint与六个 ARM64 生产程序构建通过；GitHub CI 38563992317 success。NAS Go 1.26.5、CGO_ENABLED=0 的 ARM64 客户端构建和测试通过，包含原协议五组签名黄金向量。构建使用独立有限资源容器，无网络下载；没有在棒子上编译。

后续源码530e6f5dcd3f1510fc4057c4b7dce141669240e8增加 observing 只读模式及联合验收脚本，源包 SHA256 0c14aad8c5ff9fc9790982c09baba38b761c6f2d72715903b6d231214e2c6ccc。163项测试、六程序 ARM64 构建通过；GitHub CI 38565794201 success。本轮增量 ARM release 34.24秒，不是冷缓存全量构建速度对照。

## 实际通过的边界

- 空业务库和合成历史使用同一个组合 Core；兼容授权目录独立，旧认证表没有导入。
- 新 owner 明确批准独立 NAS 公钥和当前 generation，再绑定只读 scope。设备身份私钥没有复制到 NAS。
- 实际 Go 客户端校验 CA、hostname、固定设备公钥与绑定 epoch，申请短期会话并读取状态、短信列表和通话列表，响应明确为 data_format=2。
- 兼容 token 不能访问 v2；v2 token 不能访问兼容读取入口。
- 捕获的签名请求第二次被判定重放。撤销映射立即拒绝旧 token；重新绑定改变 epoch，旧签名不能复用。
- Core 重启后旧 token 拒绝、已使用 nonce 继续拒绝、新签名会话成功。主 NAS principal 撤销后申请会话被拒绝。
- 合成历史的来源/用户隔离、tombstone、显式授权、撤销/generation和重启验证同时通过；归档 hash 不变，没有产生历史业务重放。
- Core/Edge 真实非 Root UID 和空能力集合通过。自有临时单元停止，原四服务 active、PCM closed。没有发送短信、拨号或蜂窝流量测试。

私有联合报告 `.private/nas-compat-d20b9af-first.json` passed=true、cleanup_verified=true，两个 Go 会话/撤销/重启验证字段为 true。真实收件箱的 observing 验收须以独立实际报告为准，不能由空库读取推定成功。

## 真实收件箱的只读桥

530e6f5产物完成同一组合验收后，切换到 `--serve-observing-compat`，只开放实际 modem 观察，Core层关闭写接口。真实短信列表与每条详情由NAS Go客户端读取，逐项与当前owner授权的v2响应比较一致；真实内容只在私有进程/目录处理，没有提交。测试没有发送短信、拨号或激活普通APN。

owner的短信准备请求被只读运行模式拒绝；撤销兼容映射后NAS读取立即拒绝，owner本地收件箱保持一致。实际Core/Edge非Root/空能力集合通过，所有自有测试服务停止、旧四服务active、PCMclosed。

私有报告 `.private/nas-compat-530e6f5-live-first.json` passed/cleanup_verified=true；actual_inbox_bridge_equal_to_authorized_v2、actual_readonly_controls_denied、nas_revocation_keeps_local_owner_data均true，hardware_mutations=false。原数据的线上读取与离线历史迁移是不同测试，不能将本轮结果写成原NAS数据库已在线挂接。

P5本轮增加1.5点：有限Go/NAS会话桥1点，实际收件箱只读一致性与独立权限验收半点。P5累计5/8，总体65.5点、约66%。原NAS页面、PCM和完整兼容仍未完成。

接口和限制见 [nas-online-bridge.md](nas-online-bridge.md)。这是最小读桥，不等于旧 NAS GUI 已接入、双向声音完成、正式自启安装或完整固件刷写。
