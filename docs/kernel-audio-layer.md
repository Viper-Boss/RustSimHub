# 完整系统中的固定音频驱动层

2026-10-17。Rust业务、Native用户态与Linux内核驱动分别保留来源。已逐字节重建的22个UFI003模块属于不可变OS层，不进入普通应用签名槽位，也不新增远程模块加载接口。来源/许可证和固定编译器见[KERNEL-PROVENANCE.md](KERNEL-PROVENANCE.md)及firmware/kernel/audio/modules/source.json。

`compose-system-tree.py`包含Native时必须同时显式传`--audio-modules`。输入目录只允许固定22个独立普通文件，逐个核对长度/SHA、ARM64 ET_REL、name/srcversion/vermagic/depends；额外文件、缺失、链接、篡改或不同来源拒绝。所有外部输入检查完成后才复制到新候选的`usr/lib/modules/<固定release>/updates/rustsimhub-audio`，Root/0444；旧base和旧候选不变。

离线depmod只作用于新树。随后以`modprobe -C /dev/null -d <候选> -S <固定release> --show-depends --ignore-install`查询全部22个模块；不得加载模块或执行host安装钩子。每个音频依赖必须选中updates层，不能混入同名stock；四项设备树别名也必须唯一解析。实际固定四组预加载的音频依赖集合必须与源码锁的16项closure一致。公共来源/解析结果及16项运行时profile安装到usr/share/rustsimhub，并纳入候选树摘要。

启动时仅媒体服务Requires/After有限的`rustsimhub-audio-drivers.service`。该root oneshot在私有设备固件准备后执行：核Root/0444/单链接profile及实际kernel release，使用固定四个modprobe目标/禁用外部配置，再核16个已加载模块的srcversion。仅保留CAP_SYS_MODULE，运行有30秒上限，不打开PCM。失败阻止媒体启动；Core不依赖音频驱动服务，仍可提供短信和通话控制。真实Media用户仍为空能力集合，不能自己加载模块。

旧系统的`test-boot-service-group.py`继续只验证用户态，不能调用未改名的宿主驱动服务。新版必须显式`--verify-preloaded-audio`，先只读核对实际kernel和16项现存srcversion，才在改名试验单元中以该前置证据替代驱动服务依赖；报告明确audio_driver_loading_tested=false，不把这种测试冒充新OS加载或冷启动。先前4e6/P6K证据仍属于旧依赖图。

新离线候选P6M已纳入22个驱动，目标ARM modprobe的22个依赖图/四个别名、七ARM程序和全服务静态依赖通过，树摘要保持；见[验收](VALIDATION-KERNEL-AUDIO-P6M.md)。原P6L树保持。源码锁中的installed_in_system_candidate=false表示该来源锁本身不证明安装；实际安装状态以每份SYSTEM-CANDIDATE.json/验收报告为准，不动态改写来源锁而重标旧制品。

当前不能凭隔离夹具和离线解析宣称完整新系统启动或音频驱动服务实机通过。flash/release/recovery/board_boot门禁保持false，蜂窝流量等待换卡。
