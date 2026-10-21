# Rust 依赖公告复核 P7D

2026-10-21。使用cargo-audit 0.22.2，分别检查实际应用来源4e6fea6的workspace与Native两份Git原始锁文件。RustSec数据库固定到 `e6f94f3760d5e6c54c1b05f204002263bc025524`，并通过独立GitHub API确认该提交存在于官方库；第二次复核使用同一数据库，不忽略公告、不自动改依赖。

## 发现和修正

旧workspace命中5条公告：rustls 0.23.45的[0346](https://rustsec.org/advisories/RUSTSEC-2026-0346.html)、[0347](https://rustsec.org/advisories/RUSTSEC-2026-0347.html)、[0348](https://rustsec.org/advisories/RUSTSEC-2026-0348.html)、[0349](https://rustsec.org/advisories/RUSTSEC-2026-0349.html)，以及rustls-webpki 0.103.15的[0382](https://rustsec.org/advisories/RUSTSEC-2026-0382.html)。只将两项锁定依赖更新为0.23.46与0.103.16及对应registry校验和；其余版本和Native锁文件保持。修复范围与[上游发布说明](https://github.com/rustls/rustls/releases/tag/v%2F0.23.46)相符。

相同数据库复核：workspace与Native漏洞命中均为0。格式、209项workspace测试、Core生产功能集合和Clippy通过。没有用版本断言测试替代真实TLS/权限/租约等既有回归。旧扫描失败报告保持，不能将新锁文件结论覆盖给4e6fea6已编译程序。

源码目前使用Edge TLS服务端、`with_no_client_auth()`，配置没有启用Brotli证书解压；没有发现公告所述自定义客户端会话Debug/ECH路径。该源码检查仅说明当前功能边界，仍采用完整上游补丁，不将公告全局忽略。

## 维护警告与后续

workspace仍有5项unmaintained警告，未忽略：basic-toml、proc-macro-error2、zbus_names、zvariant及zvariant_derive。后面三项已被上游合入zbus 6，需要独立迁移并复核真实ModemManager/NetworkManager接口与权限拒绝路径；前两项需要追踪构建期依赖链。详见[RustSec zbus_names说明](https://rustsec.org/advisories/RUSTSEC-2026-0371.html)、[zvariant说明](https://rustsec.org/advisories/RUSTSEC-2026-0370.html)、[basic-toml说明](https://rustsec.org/advisories/RUSTSEC-2026-0374.html)及[proc-macro-error2说明](https://rustsec.org/advisories/RUSTSEC-2026-0173.html)。这些记录不代表已迁移或已清除所有维护风险。

这轮仅覆盖Rust锁文件，不覆盖Linux、系统包、C库或全部应用逻辑。私有证据前缀 `rustsec-audit-20261021-first/second`、`tls-security-tests-20261021-first.log`；复制输入和前后摘要保持。审计网络访问发生在本机，不使用棒子流量。

## ARM及完整候选验证

修正源 `3da467944d47ec526d44356e3dd946782510c6d6` 的[完整CI39546547484](https://github.com/Viper-Boss/RustSimHub/actions/runs/39546547484)成功。精确115文件归档SHA为 `e779ef771033f2c47d3bf4e60e0660cdb5983bf293f6eb87b32d1cac9781b81a`；本机Rust1.98/12任务六生产ARM程序构建退出0，架构与glibc边界通过，六程序在NAS Debian13非Root/无网络/无设备容器执行来源信息和Core无硬件自检通过。Native在固定ARM镜像以实际Rust1.99构建，21项测试及源码摘要/worker能力通过；退出0后收集产物并移除唯一临时容器，缓存/源码/日志保留。

使用此前4e6已选签名密钥签署新epoch4/5/6包，没有重新生成或上传私钥。独立通用候选使用epoch4，包含恢复单元完成条件及两项TLS补丁；七个ARM非Root执行、静态单元依赖、源树保持、完整ext4逐项核对/只读FS通过。12367项，tree `0508cc1e5d3beb95ab10cd540475042e472ee9e64912f5dc5a4978e8b81edf78`，3436183552字节，image SHA `c4c34a36512f4fe4e4b9309aa1828e868318788a72a1d2f42eb0999d091032e5`。私有manifest/验证前缀为 `system-image-p6s-tls-security-3da4679-first` / `system-tree-p6s-tls-security-3da4679-first`，完整索引 `tls-security-build-20261021-first.json`。启动前因私有Shell换行错误未开始组合的失败也保留，修正LF后才创建输出。

**该新候选未部署，实机TLS尚待验收。**正在安装的bbb1081镜像仍包含4e6fea6应用，不能记为已更新；先完成该次安装和维护/音频诊断，再决定新通用候选的最终修正并在启用业务前纳入修复产物。整体保持74/100，flash/release/boot/recovery门禁保持false。
