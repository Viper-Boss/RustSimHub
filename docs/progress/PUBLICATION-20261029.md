# 2026-10-29 独立公开看板发布

用户明确选择“单独公开看板，固件仓库保持私有”。已创建独立仓库 [RustSimHub-progress](https://github.com/Viper-Boss/RustSimHub-progress)，仅放13个明确允许的静态页面、任务快照、页面源码/重建脚本和许可证文件，没有导出固件仓库历史、业务源码、凭据、设备资料或原始日志。

- 正式网址：**https://viper-boss.github.io/RustSimHub-progress/**
- 固件仓库 `Viper-Boss/RustSimHub` 保持私有，未合并任何产品PR、未操作设备或Claude工作区。
- 公布的五人并排快照来自 `bcefada0d3c04d7a8523a71859d8f2b3441b0d95`（PR58），原固件81.5%、行动20/169、D20/26离线交付保持；这不等于PR58已合入固件main。
- 公开仓库提交 `c6f31b35ec6b7666da3a16abe729011dc8cc5903`；Pages源为main根目录，HTTPS开启。
- Pages构建1322143725为built；[运行40735735308](https://github.com/Viper-Boss/RustSimHub-progress/actions/runs/40735735308)的build/deploy/report均success。
- 北京时间2026-10-29 01:28实际以正常HTTPS、带缓存区分参数下载index.html、progress.json、remaining.md和publication.json，全部成功；页面/JSON/Markdown摘要逐项匹配发布清单，页面摘要 `f58a5ebdb50968138bf5e037a110911bc2781e38adeccfe3e55a87ad7d8573a4`。

Windows换行导致最初发布清单中的三个文本摘要与Git规范化后的字节不同，首次上传前已补LF规则并修正，逐个核对Git实际12个内容文件摘要、共13文件通过；没有带错摘要上线。

工具创建线上浏览器tab的调用超时，重新读取清单确认该线上tab已经存在；后续绑定仍超时，因此本批只证明实际HTTP内容/部署与摘要，不声称完成截图、真实详情点击或焦点验收。私有验证文件保存在 `.private/progress-pages-20261029/`。

后续更新只从经过验证的固定源码提交导出明确允许文件到 `.publish/RustSimHub-progress` 独立公开目录，重建/摘要核对后正常推送，再等Pages built并下载缓存区分版本核验。不要将固件仓库设为public，不复制整个仓库或私有验证目录。页面内指向私有证据的链接仍要求登录授权。

原 `https://viper-boss.github.io/RustSimHub/` 未配置，仍不是看板入口；对外应使用上面的独立公开网址。当前README、看板说明和Markdown生成器已同步新入口，历史记录中的旧地址不代表已配置重定向。
