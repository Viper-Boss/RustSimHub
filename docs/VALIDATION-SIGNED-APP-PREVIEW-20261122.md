# 签名应用包网页预览验证

2026-11-22。代码在 `codex/signed-application-package-20261122`，已接入应用 113 的短信转发单登录修复。

- 网页“设置 → 关于”允许当前所有者明确选择并上传 `.rshapp`。Edge 先核对主界面的会话，验签完成后复核会话。单次上传有大小和时间限制，服务从固定的系统发布公钥读取信任，只显示验过签的版本和包内数据库范围。
- 本机管理员命令 `rustsimhub-update admit-stage-file STORE RELEASE_PUBLIC PACKAGE.rshapp` 保留原始签名内容，继续使用既有数据围栏、版本下限及候选准入。格式或签名错误在打开写入账本前拒绝。
- 本机对 113 的现有签名包实测。单文件 24,584,586 字节，版本、清单摘要、源码摘要及数据库范围一致。已确认 113 的隔离账本拒绝 113 再次暂存，前后账本一致。损坏包被拒绝，空账本目录保持空。
- `node tools/test-web-update-package.mjs` 通过选择、显式上传、回执显示、拒绝和退出后抑制迟到响应。Edge 实际处理器测试覆盖跨来源、无令牌、权限拒绝、超限、篡改和验签后撤权。Core 实际 IPC 测试覆盖所有者、有更新权限的普通客户端和无效令牌。
- WSL `cargo test --locked --offline -p rustsimhub-update -p rustsimhub-core -p rustsimhub-edge --lib --bins --features rustsimhub-edge/airsim-voble -- --test-threads=1` 共 431 项通过，19 项既有忽略。初次并发运行有一个既有 outbox 写锁隔离用例未通过，串行重跑通过。生产 Clippy 严格检查通过。

网页安装确认、持久回执与启动现有维护任务仍待接线。更新包入口未安装到棒子，棒子仍确认运行应用 113。整体验收 81.5%，台账 20/169。
