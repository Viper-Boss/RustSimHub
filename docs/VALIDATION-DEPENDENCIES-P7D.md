# Rust 依赖公告复核 P7D

2026-10-21。使用cargo-audit 0.22.2，分别检查实际应用来源4e6fea6的workspace与Native两份Git原始锁文件。RustSec数据库固定到 `e6f94f3760d5e6c54c1b05f204002263bc025524`，并通过独立GitHub API确认该提交存在于官方库；第二次复核使用同一数据库，不忽略公告、不自动改依赖。

## 发现和修正

旧workspace命中5条公告：rustls 0.23.45的[0346](https://rustsec.org/advisories/RUSTSEC-2026-0346.html)、[0347](https://rustsec.org/advisories/RUSTSEC-2026-0347.html)、[0348](https://rustsec.org/advisories/RUSTSEC-2026-0348.html)、[0349](https://rustsec.org/advisories/RUSTSEC-2026-0349.html)，以及rustls-webpki 0.103.15的[0382](https://rustsec.org/advisories/RUSTSEC-2026-0382.html)。只将两项锁定依赖更新为0.23.46与0.103.16及对应registry校验和；其余版本和Native锁文件保持。修复范围与[上游发布说明](https://github.com/rustls/rustls/releases/tag/v%2F0.23.46)相符。

相同数据库复核：workspace与Native漏洞命中均为0。格式、209项workspace测试、Core生产功能集合和Clippy通过。没有用版本断言测试替代真实TLS/权限/租约等既有回归。旧扫描失败报告保持，不能将新锁文件结论覆盖给4e6fea6已编译程序。

源码目前使用Edge TLS服务端、`with_no_client_auth()`，配置没有启用Brotli证书解压；没有发现公告所述自定义客户端会话Debug/ECH路径。该源码检查仅说明当前功能边界，仍采用完整上游补丁，不将公告全局忽略。

## 维护警告与后续

workspace仍有5项unmaintained警告，未忽略：basic-toml、proc-macro-error2、zbus_names、zvariant及zvariant_derive。后面三项已被上游合入zbus 6，需要独立迁移并复核真实ModemManager/NetworkManager接口与权限拒绝路径；前两项需要追踪构建期依赖链。详见[RustSec zbus_names说明](https://rustsec.org/advisories/RUSTSEC-2026-0371.html)、[zvariant说明](https://rustsec.org/advisories/RUSTSEC-2026-0370.html)、[basic-toml说明](https://rustsec.org/advisories/RUSTSEC-2026-0374.html)及[proc-macro-error2说明](https://rustsec.org/advisories/RUSTSEC-2026-0173.html)。这些记录不代表已迁移或已清除所有维护风险。

这轮仅覆盖Rust锁文件，不覆盖Linux、系统包、C库或全部应用逻辑。私有证据前缀 `rustsec-audit-20261021-first/second`、`tls-security-tests-20261021-first.log`；复制输入和前后摘要保持。审计网络访问发生在本机，不使用棒子流量。

新ARM构建、签名制品及实机TLS验收尚待完成。正在安装的bbb1081镜像仍包含4e6fea6应用，不能记为已更新；先完成该次安装和维护诊断，再在启用新业务前纳入修复产物。整体保持74/100。
