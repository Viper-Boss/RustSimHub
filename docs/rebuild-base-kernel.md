# 固定上游基础内核复现

该工具用于从完整固定源码树重建基础内核及原装模块，输出新目录中的普通文件，不生成Android boot、不访问棒子、不部署或加载模块。

**当前状态：整合工具已完成真实构建验证。**8项工具夹具、139项完整系统工具回归全部通过；新second输出约181.3秒完成，两种内核格式及681个打包模块逐字节匹配，完整源码树保持。详见[真实结果](VALIDATION-FULL-KERNEL-P6T.md)。

## 准备输入

- `upstream-source.lock.json`固定的完整源码：先用 `verify-kernel-source-tree.py --archive <orig.tar.gz> --report <新报告>`核完整Git树，再在私有目录安全解包。工具再次计算解包树，必须精确匹配93a71ee的根tree；不要在带.git或额外文件的目录编译。
- 固定image包内的原config，摘要见 `base-build-profile.json`；固定上游boot.img和linux-image Debian包。三者均在执行编译前核对完整摘要，模块包仅流式读取，不能把它安装到宿主系统。
- 私有Debian GCC14.2.0-19、binutils2.44和ARM64 sysroot，入口名为 `aarch64-linux-gnu-gcc`。原版gcc-14可在私有SDK同目录建立指向它的gcc别名；不修改系统工具。完整原配置、release和输出字节都必须匹配，不能删去RELR来迁就旧binutils。
- 私有native bison/flex/m4、libelf/OpenSSL头和库，以及make、native GCC/G++。工具记录被选可执行文件摘要；版本修订相同不能解释为编译器二进制与上游完全相同。

上述路径和输出使用没有空白或shell语法的规范绝对路径。所有输出必须新建且与源码树分离；失败目录保留，不自动清理或覆盖输入。建议使用私有Linux构建目录，避免将历史输出、原镜像或设备状态用作可写工作区。

## 执行方式

```sh
python3 tools/rebuild-base-kernel.py \
  --source /private/linux-source/linux \
  --config /private/original.config \
  --reference-boot /private/boot.img \
  --image-package /private/linux-image-fixed_arm64.deb \
  --toolchain-bin /private/debian-sdk/prefix/usr/bin \
  --sysroot /private/debian-sdk/prefix \
  --toolchain-runtime /private/debian-sdk/runtime-libs \
  --host-prefix /private/native-tools/prefix \
  --host-runtime /private/native-tools/runtime-libs \
  --make /private/build-tools/prefix/usr/bin/make \
  --output /private/new-base-rebuild --jobs 12
```

工具使用固定上游元数据仅用于来源复现，不把历史构建主机/用户冒充为本次执行环境。附加MAKEFILES分别设置临时弱横幅的空时间、内置cpio的原时间和最终版本时间；不改源码。配置有任何变化、工具链版本不同、已有输出、源树变化、内核字节不同、模块路径集合或打包字节不同均失败。

成功输出应包括 `BUILD-REPORT.json`、`build.log`、`metadata.mk`、原始 `build/`、新副本 `packaged-modules/`及全量 `PACKAGED-MODULES.json`。仅同时满足内核两种格式、681个模块及源保持，才可称基础内核复现通过；这些文件不含整机首启、外部initramfs、冷启动、恢复或发布结论。

验证命令：在Linux下运行 `python3 -m unittest discover -s tools/system -p test_base_kernel_build.py` 和 `python3 -m unittest discover -s tools/system -p 'test_*.py'`。真实挂载隔离夹具需要Root及包含sbin的PATH；完整构建需要上述私有输入。此前审批服务额度错误已在同一正常审批路径恢复，保留历史失败，不曾绕过审批或用CI代跑受阻动作。
