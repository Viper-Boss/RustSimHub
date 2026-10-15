# 统一开机应用组

组合审计补充：首轮ed78621更新器实际187项及六程序ARM生产构建完成，但候选树被旧polkit单规则审计拒绝；失败目录保留。审计现以固定路径/精确SHA256识别新Rust规则，权限可写、链接、增加动作或改变subject均拒绝，不放宽旧单规则解析器。新增一项审计测试，rootfs测试34项；仍需重新组合及实机验收。

2026-10-15源码检查点：下列启动路径已经实现，尚待冻结构建、系统组合和实机单元验收。当前不能据此称完整系统已启动，整体仍69/100。

`rustsimhub-app.target`统一要求select/Core/Edge。只有select执行一次`rustsimhub-update boot-select STORE PUBLIC FACTORY RUNTIME PHYSICAL_VERSIONS`；它先检查versions绑定的device/inode相同，再验证签名factory并初始化缺失基线、choose一次、发布Root控制的selected链接。Core/Edge各自只读绑定同一个已选择目录，不分别choose。已有异常链接/普通文件和错误versions绑定拒绝，未消费候选；合法遗留pending可在下一次选择回退时清理链接，不删应用文件。

台账位于NSHSTATE的system/app，payload位于根系统分区/opt/rustsimhub/versions，通过select私有mount namespace嵌套绑定到Store/versions；避免把多份15MB应用存入小持久分区。整个rootfs重刷时必须保留或迁移台账所指的应用版本，当前没有整机升级/自动降级路径。

systemd-tmpfiles创建本机IPC目录，Core986/IPC987/2750。Core/Edge分别将自己的持久叶目录映射为独立路径，原始持久树被屏蔽，空cap、PrivateDevices、NoNewPrivileges、只读系统。Core只允许AF_UNIX，Edge允许本地HTTPS；Edge屏蔽D-Bus。有限polkit规则仅给Core实际Messaging/Voice及Wi-Fi所需权限，其余项目账户硬件控制拒绝；Messaging/Voice动作名称已从实机pkaction核对。

Core仅初始化空历史归档或检查既有归档，不自动准备设置码、不重开窗口；Edge等待Core套接字并读取已由可信安装层提供的私有TLS/config。当前安装层及正式设置码/证书交付仍待完成。媒体常驻进程/信令尚未接入，不宣称媒体可用。

核心或Edge失败会让整组退出，恢复服务先持久写入本次开机的once标记，再调度统一target重新启动。未确认候选由选择器回退，已确认版本最多重试一次；第二次失败停留以待维护，不反复启动。此处不会自动确认应用健康，候选仍需可信业务检查后confirm；不创建未经owner批准的健康客户端。

`compose-system-tree.py`新增成套可选输入`--signed-application --release-public --updater-artifacts`。使用OpenSSL核验与Rust相同域的精确manifest签名，并核对固定文件列表/大小/ELF/摘要/源码、重复字段及类型。复制后再次验证签名包与更新器摘要，要求新更新器声明支持统一开机选择，随后才启用target并记录app_service_group_installed=true。没有成套输入仍不启用。该字段仅表示安装，不表示开机验收；flash_allowed/release_ready/board_boot_verified保持false。

更新器新增两项失败路径测试（组件共11项）；板级35项含OpenSSL签名/畸形manifest/错误source/链接及自动恢复once边界。`package-development-application.py`只在.private生成同源生产二进制的开发签名包和开发seed，不能当生产信任发行。主应用/Native可保持9b5f1d9，更新器工具使用后续独立源并分别记录归属。
