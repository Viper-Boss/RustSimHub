# P3A 音频基础验证

日期：2026-10-13。整体约 35%，口径见 PROGRESS-SCALE.md。
Rust 源码检查点 `458a272372b9b1910af8e0ecbce7831a14700db7`；随后仅补充工具输入隔离和验证/接手文档。

## 已完成检查

- Windows：workspace 与 native/audio-alsa 分别 cargo fmt --check 通过；缺 MSVC link.exe，未在 Windows 声称编译通过。
- NAS ARM64：默认 workspace check、all-features test、release、自检和 CLI help 通过，构建容器 exited=0。
- 102 项 workspace 测试：auth 21 / core 22 / ctl 1 / domain 9 / media 12 / policy 1 / storage 31 / wire 5。
- 新媒体测试覆盖单次票据、错误 peer、过期、租约上限、core 丢失、身份/权限/呼叫代次变更、分段 I/O、固定 LE 帧、满队列/过期/乱序、撤销前止 I/O、时钟倒退、EAGAIN、xrun 上限。
- 原生 ALSA 单独 4 项测试通过：null 双向格式/取消、suspend 拒绝、精确 PCM ID 匹配、失效授权在任何硬件探测前拒绝。
- 合计 106 项；null PCM 与模拟设备不证明真实电话有声音。
- [源码 CI](https://github.com/Viper-Boss/RustSimHub/actions/runs/38348798372) 成功：Rust 1.98 的 fmt、102 项测试、默认 core check、workspace Clippy -D warnings。原生 crate 被 exclude，不在这次 Clippy 范围内；原生 Clippy 尚未运行。
- 棒子运行独立非 root 一次性探测单元，profile_matches=true / routes_on=7/7 / pcm_opened=false / mixer_written=false。
- DevicePolicy=closed，仅放行 controlC0；PCM 节点不在 DeviceAllow 中。ProtectSystem=strict / NoNewPrivileges / RuntimeMax=15s。
- 探测前后所有 PCM 状态 closed；旧 nassimhub-agent、MM、NM、rmtfs 均 active。探测单元执行结束并 collect，没有新增拨号、短信、流量连接或分区写入。

## 修复记录

首次镜像 apt 索引因 BuildKit DNS 失败；改为 Debian HTTPS 包索引选定版本、锁 SHA-256，下载与 offline 镜像构建分别验证。
原生测试首次遇到 IoError 缺 Debug，仅给有限错误枚举补充 Debug；语音帧/票据/授权快照仍没有 Debug。
初次实机探测把空 PCM 显示名称当标识；改为 ctl PCM ID，匹配有限名称及 ` (*)` 后缀并新增测试。
首次 CI 的 chunks_exact 固定长度 lint 已按 as_chunks::<2> 修复，最终 CI 成功，无 allow 绕过。

## 源包、构建器与产物

允许列表源包 60 个文件逐字节与检查点 Git blobs 相同。
core 与 native 最终构建均使用此源包 SHA-256：
`77722e100e2ca561b04371a194e518ad1d6ca6331662080103d7de4447de763d`。
源包只包括 Cargo 清单/锁、LICENSE/NOTICE、crates Rust/SQL 及 native/audio-alsa Rust；没有 target、凭据或私有报告。

| 产物 | 字节 | SHA-256 |
|---|---:|---|
| rustsimhub-core 开发包 | 5264864 | e57613ad4f30e6f6f71ca32a00f6e65368b1f2bcb086837d5cf76962f53840b8 |
| simhubctl | 857104 | 39cad87bb1d092c0aee001fbf6e84a546ace924750be037e6242fe6b67e33422 |
| rustsimhub-audio-alsa 只读探测程序 | 332616 | 813ddfd5569f74f5d75ea9c8d48c648eb4d81c497e7b2bd8edf3bc178155253f |

core/CLI 未接媒体，因此哈希与通话阶段一致；core release 含 dev-fixtures，不是生产镜像。
音频程序 ELF ARM64 PIE，程序本身所需符号最高为 GLIBC_2.34、动态链接 libasound.so.2；在当前 Debian 13.7 棒子已加载运行。

基底仍为 `rust@sha256:2c3a22f0a5533ea2dd5a16627bc841228151faa2d4de2644ac9987e4a2f1f2fa`。
音频派生镜像 ID `sha256:5a4a245b4ec7387f761b87a7ffefbc2ad2b7ee047ca2297c3b066ff75dedba0c`。
Rust 1.99.0 / libasound 1.2.8；棒子 libasound 1.2.14，未在 NAS 或棒子安装开发包。
native/audio-alsa 单独 Cargo.lock；Debian 开发依赖锁见 tools/audio-builder-deps.json。

本机音频产物 `.local/artifacts/rustsimhub-audio-alsa-p3`；棒子 `/opt/rustsimhub/dev-audio-p3/rustsimhub-audio-alsa`，传输前后 SHA-256 一致。
NAS `/mnt/docker-data/rustsimhub/p0/audio-target/release/rustsimhub-audio-alsa`，专用容器 rustsimhub-audio-build-p3 已退出 0。
源码位于独立只读 audio-src，缓存 audio-cargo，core build 也已退出 0。
后续上传音频源包使用 audio-upload.tar.gz，与 core source.tar.gz 分开，避免覆盖已完成构建的输入证据。

完整报告留在本机 `.private/audio-readonly.log`、`audio-native-metadata-final.log`、`audio-native-final-build.log`、
`audio-native-install-final.log`、`audio-native-device-probe-final.log`、`audio-core-metadata-final.log`、`audio-core-final-build.log`。
不复制私有日志到 Git。

## 尚未验收

Authority/Permit 只是受信组件原语；core 尚未从真实 Principal/CallLeases/Binding 构造它，也没有跨进程 UDS/票据接口。
真实 CS-Voice holder、媒体 PCM 同时打开及精确 period/buffer 尚未验证；读取七项开关不代表实际样本或声音可用。
没有真实采集/注入、听音、Opus/WebRTC/ICE/TURN、量化延迟、长通话或 xrun 实机证据。
本轮没有拨号；此前一次 10010 授权已使用，进一步实机通话要新的明确授权，不能删除旧 once 标记。
完整系统、冷启动路由初始化、生产媒体进程的最小权限和恢复/刷机资料仍未完成。
