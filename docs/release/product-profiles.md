# 有限产品发行配置：构建、启动与兼容边界

实现入口：`tools/product-release/release.py`；launcher：`crates/core-launcher/src/product.rs`；独立材料：`firmware/system/product-profiles/`。本工具构建程序、检查实际元数据并生成未签名材料，不安装、不启用服务、不迁移、不分配epoch。

## 配置与旧默认

| profile | Core新增启动前缀 | 邮件outbox/worker | BLE恢复 |
|---|---|---|---|
| `legacy-v1` | 无 | 3 / 4 | 不启用 |
| `product-v1` | product providers | 3 / 4 | 不启用 |
| `product-email-v1` | product providers | 4 / 5 | 不启用 |
| `product-recovery-v1` | product providers、secure recovery | 3 / 4 | 显式候选，要求独立schema9准入 |
| `product-email-recovery-v1` | product providers、secure recovery | 4 / 5 | 同上 |

不接受任意feature、命令、UID、接口名或服务列表。产品profile编译原有Core provider、SMTP（选中时）、Go受管SDK/Root和Rust BLE host，不重写这些业务。NKN仍含已有Go SDK，不称为全Rust引擎。原生音频采用默认LAN构建；此工具没有宣称TURN媒体已正式启用。

**未安装 `/etc/rustsimhub/product-profile.json` 时，launcher继续原选择逻辑。** 明确存在但损坏、来源/摘要不匹配、越权或不支持的profile拒绝，不静默提升或降级。显式legacy配置也绑定新Core字节，不替代历史无profile的兼容入口。旧AP契约缺失仍按原规则保留基本通信fallback。

profile文件只有四项：format=1、固定profile名称、完整源码归档SHA-256、实际Core SHA-256。launcher核Root属主/权限/单链接普通文件、大小、实际Core摘要、源码和真实outbox/worker协议，再运行同一Core的有限前缀加 `--build-info`，确认它真的实现这些参数，最后保持同PID exec。外部不能在JSON里指定路径或参数。Core原来对旧schema/缺材料的拒绝保持，本包不自动创建身份或迁移schema9。

## 准备输入

1. Linux普通用户、固定Rust1.98.0及GNU ARM交叉工具链，QEMU、readelf、pkg-config、已验证ALSA sysroot。产品profile另外需要Go1.27.3、固定模块缓存和既有已验证SDK backport stage。
2. 从固定Git提交生成完整archive与SHA-256。工具复用 `arm_build_manifest.verify_source` 核完整Git blob集合；运行中的工具/模板也必须与归档同字节。
3. 自有只读NSS快照JSON，精确四字段 `passwd/group/nss_passwd/nss_group`。先跑现有AP真实身份守卫，再核BLE/NKN独立UID/GID、别名和补充组。本工具不分配数字或改NSS。离线合成快照只能证明解析/兼容检查，不是设备账户已存在。
4. 固定SDK来源按 `tools/nkn-build/sdk-managed/README.md` 准备；新工具复用其独立核验函数和全部补丁前后摘要，派生自己的新stage，不改旧输入。不从默认三行go.mod直接冒充完整SDK构建。

示例（所有路径都是操作者自己的私有构建输入，不是设备端路径）：

```sh
python3 tools/product-release/release.py \
  --profile product-email-recovery-v1 \
  --archive /private/source.tar --repository /private/linux-checkout \
  --revision FULL_COMMIT_SHA --source FULL_ARCHIVE_SHA256 \
  --roles /private/roles.json --verified-sdk-stage /private/verified-backport-stage \
  --sysroot /private/gnu/usr/aarch64-linux-gnu \
  --audio-sysroot /private/alsa-sysroot --output /private/NEW-profile
```

网络下载不属于构建器：Cargo locked/offline、Go readonly/offline。缺缓存先明确准备固定版本，失败目录保留，不能修改锁或套一个fixture SDK。SDK原验证器使用断言，工具明确拒绝Python `-O/-OO`，避免把关闭断言当校验成功。

## 实际输出

- `programs/`：默认13个真实ARM程序；产品配置另含BLE host、NKN Root和SDK，共16个。每份检查ELF、大小、GLIBC上限；可提供元数据的程序运行真实ARM CLI。SDK还核nonfixture标志、实际Go build ID、固定源码和许可闭包。
- `materials/`：Root安装后的有限profile文件、固定Core drop-in、按已核NSS渲染的目录材料，及按profile选择的独立NKN/BLE服务/策略。没有Install/WantedBy、enable链接、凭据或开启标记。
- `licenses/` 和产品配置的 `sdk-licenses/`：实际许可文本、摘要和编译模块记录。架构可能不链接旧基线模块，明确列出而不是伪称新增依赖；不接受新模块或变更版本/许可摘要。许可收集不是安全公告清零或正式依赖批准。
- `profile-build.json`、`COMPLETED`：完整来源/字节/契约/材料/许可记录。失败保留 `INCOMPLETE`，不覆盖或自动续跑。包含源码和target的私有工作目录不是可直接刷写的系统。

所有结果保持 `signed=false`、`epoch_assigned=false`、`installable=false`、`bootable_rootfs=false`、`health_authorized=false`。这不是现有签名应用manifest格式，不能给普通updater当作候选。

## 固定部署布局与权限

| 材料 | 规划目标 |
|---|---|
| profile JSON | `/etc/rustsimhub/product-profile.json` |
| Core launcher | `/usr/libexec/rustsimhub/rustsimhub-core-launcher` |
| Core drop-in | `/etc/systemd/system/rustsimhub-core.service.d/50-product-profile.conf` |
| Go Root/SDK、BLE host | `/usr/libexec/rustsimhub/product/` 下三个固定程序 |
| NKN/BLE单位 | `/etc/systemd/system/` 下相应固定单位 |
| 目录与蓝牙总线策略 | 固定tmpfiles和system bus策略位置，由正式Root写入表审查确定 |

现有应用format1/2载荷集合不能随意增加SDK。NKN单位仅在自己的mount namespace将固定Root程序目录绑定到它原来要求的selected路径，不把SDK塞进应用槽，也不改Go helper的路径接口。该mount布局仍需实际PID1/设备验证。

NKN新增角色不能加入通用IPC、audio、Core或BLE组；BLE也不加入通用IPC。目录由明确Root材料创建，Core仅拥有自身socket目录。Core保持空capability，NKN Root沿用原NET_ADMIN/SETUID/SETGID有限合同，BLE仅Unix总线和专用恢复端点。BLE策略复用已有精确规则，仅补实际backend需要的固定GetManagedObjects读，不修改Power/Pair/Trusted。

SMS转发总开关仍由原Core/worker管理，关闭时不应运行worker；编译SMTP不是发送授权。NKN手动、远程、统计三引用维持原监督器，状态查询不启动SDK，释放一个引用不代表共享进程已退出。BLE/NKN安装材料本身不创建开启标记，不开启蜂窝或Wi-Fi。

## 三方兼容与失败

用同源真实launcher的 `--check-upgrade` 做当前/候选/回退三方比较，不能只比候选：

```sh
python3 tools/product-release/compare.py --launcher /private/programs/rustsimhub-core-launcher \
  --sysroot /private/gnu/usr/aarch64-linux-gnu --request /private/versions.json
```

输入精确沿用旧launcher兼容请求：当前、候选、回退各自的实际Core/worker stdout；Root模式与契约、实际business schema/outbox版本。CLI保持0=compatible、3=maintenance_required、4=rejected；无论哪种结果都不是更新授权。本工具不从网页JSON推断真实数据库版本，也不打开业务writer。

邮件队列3→4后，旧worker4不能读取format4，不能按“两个程序都能构建”就宣称安全回退。即使原outbox缺失，候选可能创建新格式，仍检查回退。schema9是独立执行模式；Core通用build-info的base schema6不能拿来批准已有schema9数据，必须由正式专用准入接续。被拒时不迁移、写账本、清floor或替换身份。

## 自动确认、安装与恢复

窗口3拥有固定健康端点及 `confirm-running-candidate`；本包不生成不存在的健康启动参数，不调用裸 `confirm HASH`。当前产物明确不支持自动健康确认。接口交付后需按完整新来源另出有限profile，不能仅改JSON宣称已接通。

正式安装前仍须新鲜只读设备证据、匹配的业务/Root配置备份、明确恢复预算、全部签名字节、新角色与固定写入目标审查。这里没有修改旧八目标Root维护白名单；新增目标不受旧包授权。任何失败保留原组和数据，不自动刷分区或恢复旧数据库。真实systemd/cgroup、BlueZ/RF、实际NKN网络、邮件递送及设备冷启动由主线另行验收。
