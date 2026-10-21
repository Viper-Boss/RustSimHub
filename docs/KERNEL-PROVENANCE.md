# 当前内核与音频DTB来源对照

2026-10-21最新：固定基础内核两种格式和681个原装模块（新副本去调试符号）均已逐字节重建，源树/配置保持。目标专属时间输入及真实证据见[完整内核P6T](VALIDATION-FULL-KERNEL-P6T.md)。仓库整合工具也已通过8项夹具/139项系统回归及真实181.3秒完整构建，两种内核格式与681个模块匹配；外部initramfs、真实新内核启动和恢复门禁保持未完成。

2026-10-21较早补充（由上节覆盖）：第五/第六份完整固定源码构建成功，681模块、15468项导出表匹配、来源树保持，当时尚不与上游内核逐字节相同，未上机。详见[完整内核构建 P6T](VALIDATION-FULL-KERNEL-P6T.md)。下文“未独立重建完整内核”为保留的历史边界，完整外部initramfs/新内核真实启动和恢复仍待完成。

2026-10-17。boot/DTB重建不访问棒子；后续额外模块只读捕获设备文件与已加载srcversion，再于本机重建。没有安装/加载新模块或写分区。board.toml的kernel_source_commit仍为UNRESOLVED：本节证明已记录boot备份中部分组件的对应关系，不替代完整boot/模块来源与恢复验收。

固定上游为[KyonLi内核提交93a71ee9468d](https://github.com/KyonLi/ufi003-kernel/commit/93a71ee9468dd8d47a9e082e5d818608f08712e3)，GitHub API标记提交签名verified；其Git tree为72d4e8e42649f763dd969a1de893051ed64387a1。[6.12.49-1发布](https://github.com/KyonLi/ufi003-kernel/releases/tag/6.12.49-1)的boot、完整orig源码、Debian源码补充、dsc/buildinfo、headers/image包共七项均已按上游SHA256核验。公开输入锁见firmware/kernel/upstream-source.lock.json，未提交二进制或设备备份。

| 对照项 | 结果 |
|---|---|
| 已记录boot备份 | 整体摘要与既有board.toml记录一致 |
| gzip内核 | 与上游boot逐字节相同；SHA256 1fbbef6ccaff103af9dc3281aacaa3d81e82e001d5957c4fa6a018faed1f767f |
| 解压ARM64 Image | 相同；SHA256 3474aab48d9ff0c9aecfd70201d932a0520f0c93ad416044711ab2644ec8b004 |
| initramfs/second stage | 逐字节相同；未独立从源码重建initramfs |
| DTB | 备份区别于上游，但由固定音频源码重建后逐字节一致，52161字节/SHA256 afb1027cc0371d491349dba868bdbabb927ac87c71f8a447dc504303f93f7425 |

DTB源码来自AirSimHub固定提交ea019485e5d890a96acae357c52b94c746de837f，五项原始Git blob与SHA见firmware/kernel/audio/source.json；保持原许可证/SPDX，不重新许可为Rust业务代码。DTC 1.7.2与CPP 15.2.0离线完成重建；DTC及libfdt仅下载解包到WSL私有工具目录，版本1.7.2-2ubuntu1，未安装到系统。未复制厂商固件、NV、校准或私人设备状态。

仓库工具只接受摘要匹配的上游DTB和固定源码，输出必须新建；源码/绑定变化、输入不匹配、既有输出、重建结果不匹配都拒绝，失败目录保留。不会读取设备、重打包boot或写分区：

```sh
python3 tools/rebuild-audio-dtb.py \
  --input-dtb /private/upstream.dtb --out /private/new-dtb-rebuild
```

upstream.dtb从已核验上游boot中只读提取，相关旧解析器为AirSimHub同一提交的kernel/audio/replace-appended-dtb.py；输入DTB固定SHA为5c5a12a8237af1d5451774f497dbd75f0c610134db8ec5d9590d741871afab19。仓库新增工具在当前源码上已实际重建并cmp到备份DTB通过；44项板级回归在umask077下通过，包含源码/绑定篡改、profile越界、错误上游和既有输出拒绝。原测试夹具曾因mkdir受umask影响而失败，显式设置合成runtime0755后回归通过，生产恢复脚本未放宽检查。

私有证据在kernel-source-20261017-first：acquisition.json、boot-comparison.json、dtb-comparison.json，以及最终有界读取版本的repository-dtb-final/DTB-REBUILD.json、repository-dtb-tests-final.log/exit0。初次缺DTC和夹具权限失败日志保留；一次Windows补跑被沙箱临时目录权限拒绝，固件验收以Linux的44项及实际重建为准。不能把GitHub verified当成独立核验其签名密钥或完整本地内核重现。

后续仍须完成完整内核源码/构建、boot头/root选择的重打包配方和新系统中的模块加载。额外模块来源重建的最新证据见下节。未独立重建完整内核/initramfs，未验证完整新系统启动、物理掉电与回刷；所有flash_allowed/release_ready/recovery_verified/board_boot_verified保持false，分母与进度73.5/100约74%不变。


## 22个额外音频模块逐字节重现

22个额外音频模块已从固定KyonLi 93a71ee源码/headers与AirSimHub ea019485补丁重建，22/22逐字节等同设备现用文件。此前归档的25个qcom模块均为上游原装，不能代替这22个override；16个已加载模块的srcversion与只读捕获一致。本机私有Ubuntu GCC14.2.0-4ubuntu2~24.04.1交叉编译，补丁零fuzz；第二版19/22全字节相同，三差异仅__FILE__路径及build-id，第三版用-fmacro-prefix-map对齐原诊断路径后22/22相同。源码/headers输入未变、未安装或加载新模块。当前P6L通用树尚缺22个override，不能据旧系统通话通过宣布新系统声音可用；下一步在新候选纳入驱动并验证depmod/别名/预加载及冷启动。完整内核/initramfs独立重建、boot重打包和恢复仍缺，board.toml保持UNRESOLVED，所有刷写门禁false，不重复加分。

固定原始patch及Makefile保存在firmware/kernel/audio/modules/；source.json锁定原Git blob/SHA、68项qcom与根源码、编译器32个deb依赖和22个输出的摘要/长度/vermagic/srcversion。两个原始文件逐字节保留AirSimHub ea019485内容，默认GPL-2.0-only，原SPDX/内核声明保持；不包含模块二进制、设备状态或厂商固件。编译器只解包在WSL私有cache，未装系统包；重编译3.18秒，不在棒子编译。

复现输入使用upstream-source.lock.json中已核验的orig源码与ARM64 headers包；先核source.json各项摘要和Git blob，再对独立源码副本以patch -p1 --batch --fuzz=0做dry-run及应用。取patched sound/soc/qcom中的common.c/common.h/apq8016_sbc.c与完整qdsp6目录，以上游原始Makefile.outoftree作为模块目录Makefile。对独立headers副本运行：

```sh
make -C "$headers" ARCH=arm64 CROSS_COMPILE="$prefix/aarch64-linux-gnu-" \
  CC="$prefix/aarch64-linux-gnu-gcc-14" M="$module_build" \
  KCFLAGS="-fmacro-prefix-map=$module_build=/work/audio/stage-c/build" -j8 modules
```

prefix指私有工具链usr/bin；x86主机需已有qemu/binfmt运行headers中的ARM64 fixdep/modpost，QEMU_LD_PREFIX指私有工具链usr/aarch64-linux-gnu。重定位binutils仅补libbfd/libopcodes/libctf/libsframe的私有链接目录，不能将整套旧libc加入LD_LIBRARY_PATH。/work/audio/stage-c/build只作为诊断字符串映射，不创建/写该目录。

私有证据：active-audio-module-capture.json、module-input-preparation.json、module-source-git-verification.json、matching-module-compiler-v3.json、module-rebuild-second.json、module-build-path-differences.json及module-rebuild-third.json/log/exit0。第二版3个不同对象只有.rodata.str1.8和build-id不同；解码证明每个仅一项绝对源码路径变化，第三版映射后22/22全文件SHA一致。全部srcversion/vermagic/.text亦匹配，原source/headers未变。初次ARM helper缺loader及工具链依赖失败保留，不以最终成功抹去。

这闭合额外模块的源码到二进制对应，但没有重建完整kernel/initramfs、重打包boot、安装驱动或实际新OS冷启动。现用旧系统内的声音和P6L用户态探针不能替代这些验收；board源归属/刷写/发布门禁仍false。


## P6M后续闭环

2026-10-17 P6M：组合源075b68e、应用/Native仍4e6fea6；22个逐字节重建音频驱动已纳入全新候选。12367项/SHA c621fdbdedee685d0f6a930a949d36f2cb6dea29953d8240b4dda7bb17b95376；host及候选自身ARM modprobe的22项依赖图/四个别名一致，后者在只读/断网/无CAP_SYS_MODULE条件下运行，没有加载模块。七ARM非Root实际执行、包含音频驱动服务的完整systemd静态依赖及树保持通过；55板级/34rootfs回归通过。原initramfs为zstd，551项/100模块，无qcom音频；固定上游boot+重建DTB+root=LABEL=rootfs+补零64MiB完整复现备份SHA d5790e07…。公开核验器只写JSON不写启动镜像，实际报告passed=true；初次只替换DTB对照失败保留。新驱动守卫仅隔离夹具/静态验证，未在棒子加载或启动新OS。旧P6L树、原失败记录及所有旧制品保留，10010仍38，无新短信/蜂窝/分区写入，整体73.5/100约74%。

详细范围、公开命令和证据见[固定驱动候选验收](VALIDATION-KERNEL-AUDIO-P6M.md)。历史boot重打包待查及P6L未纳入驱动的记录已由本节覆盖，不能据此重标旧树。root参数是文件系统LABEL，不是PARTLABEL。完整内核/initramfs独立构建与冷启动/恢复仍缺，board来源门禁暂保持；全boot SHA重现不等于从源码独立重建全部内核。


2026-10-17补充：598f273完整CI通过（https://github.com/Viper-Boss/RustSimHub/actions/runs/38996792278），包含Rust测试/production检查/Clippy与板级夹具。随后对固定orig源码tar流式计算86859个文件/符号链接的Git blob、mode和目录树，1479589504字节，根tree精确为72d4e8e42649f763dd969a1de893051ed64387a1，与93a71ee提交一致；此前只核68个qcom文件的范围已补到完整源码。没有解包或执行源码。私有complete-source-tree-hash.json及.local/hash-complete-kernel-source-tree.py保留；这证明完整来源包与固定Git提交一致，不声称独立重编译了整个内核。
