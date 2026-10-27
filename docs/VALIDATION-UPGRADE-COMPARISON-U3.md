# U3 · Root/程序/数据版本的联合比较

2026-10-27，UPD-01阶段三。已实现可复用版本比较器与有限CLI，尚未将它作为完整更新授权：签名载荷绑定、实际Root盘点和数据锁下的维护准入仍需接线，UPD-01不计完成。

## 当前行为

`rustsimhub-core-launcher --check-upgrade`只读stdin，整体最多128KiB；分别限制三组Core/worker原始build-info各16KiB、Root契约4KiB。原始stdout保留为字符串再严格解析，避免先转Value导致重复字段被覆盖。输入明确提供目标架构、Root固定模式、Root契约、业务schema及outbox观察；缺少outbox观察拒绝，不把未知当“没有数据库”。

比较selected、candidate和fallback三个角色，沿用启动器已经核对的历史档位/显式能力及强类型Root契约；新能力还核对同角色Core/worker来源相等、生产feature、协议、outbox描述一致性和有限格式语义。缺少ABI声明的历史Core只证明旧启动模式，给出maintenance_required，不猜它能读3。固定device-only入口保留基础业务行为，不因可选热点契约缺失而停止电话/SMS。

关键是先计算候选可能写出的数据：旧格式1/2若迁移到3，回退程序按**3**比较；即使当前outbox明确不存在，也检查候选将来创建3后回退是否还能读取。不会用原来的2去批准只能读2的回退程序。业务schema、Root模式/契约、程序来源/架构、worker协议和outbox分别产生有限原因，不能互相替代。

输出包含实际/投影数据版本、每个角色的结论和原因，authorization_verified始终false，不回显输入字符串。退出码0=版本兼容、3=需要维护、4=版本拒绝、1=输入无效，使shell不把“成功生成拒绝报告”当允许升级。没有默认执行路径、网络接口、shell、服务控制、SQLite迁移或消息发送。无参数正常启动路径仍使用同一固定Core及参数；新增build-info能力upgrade_check_protocol=1，launcher_protocol仍1。

版本兼容不是更新授权。当前输入里的Root模式/配置和metadata必须由维护调用者从可信盘点、已验证的确切程序和持锁数据取得；不能把网页提交的JSON或旧报告直接送去stage。调用者也必须给stdin读取设置进程截止，CLI不是监听服务。

## 本机已运行的证据

冻结744文件工作归档，SHA-256 `2542641dd8c2b09f0d3cbe2a38f284a97d6ae0d28c5cc41c3cd8b4eeffc17e33`。清理Core/forwarder/launcher本包缓存，以只读源码/依赖、UID65534、空能力/NoNewPrivileges、私有网络/设备运行。`.private/upd-compatibility-u3-20261027-first/`保存结果。

- Core157、forwarder92、Wire18、launcher30，共297项普通测试；专用ENOSPC默认ignored仍单列。四包严格Clippy、workspace fmt、三份真正生产native程序构建通过。
- 11个新增比较单测覆盖迁移后回退、候选新建outbox、三历史启动档位、未知历史构建、Root缺失/畸形、基础业务模式、schema、三角色来源/协议/架构/dev错误、未知/重复ABI字段、外来/未来格式、缺观察及输入界限。原19项launcher测试保留。
- 真正生产CLI的15个用例使用实际Core/worker stdout和真实只读SQLite探针结果，验证0/3/4/1退出码；未知格式、三角色来源冲突、Root冲突、错误架构、缺ABI历史档位、重复/缺失/超限输入均按预期。比较过程不创建工作目录文件。native历史档位是明确合成fixture，不称实际历史程序；后续ARM可单独传入实际旧程序验证。
- CI已添加同一真实生产CLI步骤，使用该checkout实际tar摘要构建程序；本提交精确CI和ARM结果另记。不得把前一5c1c97a的两个成功运行移作本次结果。

## 固定提交的完整CI与ARM补验

代码HEAD `81068ab10e379d53792039c6a154bcfc53a6cabf` 的[push CI40449868904](https://github.com/Viper-Boss/RustSimHub/actions/runs/40449868904)及[PR CI40449872660](https://github.com/Viper-Boss/RustSimHub/actions/runs/40449872660)两个运行各自test/native-audio均success。两个test job实际日志都核对11个新增比较测试及15个生产CLI用例；日志还含forwarder原3个同名模块runtime测试，分开计数，不把14项混称本模块新增。

Git精确745文件归档SHA-256 `3239b03acd76d6baf4e0a16f79d0220a3f47bf0f802546b7893a09b9f0c580fb`，全新空target构建11生产ARM程序，readelf/GLIBC及实际Core/worker同源契约通过。实际ARM程序经QEMU完成19个outbox只读CLI和15个联合比较CLI；后者传入了真正旧版Core，不是修改当前JSON冒充旧版本。旧Core先独立与既有产物摘要核对，再实际QEMU读取build-info，来源归档为`2126f7d3c9fa009478027edd7a436afe55bdcabfacee679a22d837592d17fdf9`、无dev-fixtures且确实没有forwarding_runtime_contract，比较结果为需要维护。8文件Root材料同源/签名/外部公钥验签通过，仅保存在`.private/upd-compatibility-production-*20261027-first*`。

这只补齐本阶段ARM/CLI/材料证据，没有进行设备升级或Root实盘准入。后续补录证据的文档提交另有CI，不能把本代码提交的成功称为之后所有HEAD的成功。

## 仍需接线的维护入口

1. 将签名程序字节、真实Root安装模式/配置、选定与回退身份，以及业务/outbox数据观察绑定进有限维护动作，持有约束至写入完成。当前比较器不改变既有stage/choose行为。
2. UPD-02仍负责签名维护材料接线；FWD-20仍负责格式1/2/3真实回退，不能用比较单测代替设备验收。
3. 当前无设备访问、外发或普通数据操作；PMIC/关机继续搁置，四位成员任务保持。原P0–P7为80%，扩展台账14/169、加权7.3%不变，目标active。
