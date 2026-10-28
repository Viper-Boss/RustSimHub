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

## D-26进展快照更新

北京时间2026-10-29 01:46复核第二批公开快照：公开提交 `932dd377a50c8e0ecd27db0892f599d34e003f7e`，来源私有提交 `60af31a2f166e920eaf7858796fe9fae15eca73f`（PR59）。Pages构建1322180778为built，[运行40738531683](https://github.com/Viper-Boss/RustSimHub-progress/actions/runs/40738531683)success；带缓存区分参数实际下载页面/JSON/Markdown/发布清单并逐项比对摘要通过。页面摘要 `0cf608d252fc6257dd2be66049de6e0276f989723aab14acf7e659bb46e17d9c`，焦点SEC-06且状态in_progress，81.5%及20/169保持；Markdown入口已指向新网址。证据保存于上述私有目录的update-932dd37子目录。

本轮仅同步既有13文件白名单，没有扩大公开范围。生产修复仍待PR正常审核和后续实际集成，网页快照不是固件已部署的证明。补记只变本文，不重做已通过的生产测试或伪称新运行。

## D-21恢复授权管理快照更新

北京时间2026-10-29 02:27复核第三批快照：公开提交`2aa844af7e914a4f4561cdfcefe3520178e14925`，固定私有来源`4866f89e5b712f7ced079ae0f23afd03c8fee38b`（PR60）。Pages构建1322268008为built，[运行40745069157](https://github.com/Viper-Boss/RustSimHub-progress/actions/runs/40745069157)completed/success。页面、JSON、Markdown和发布清单带独立缓存参数实际HTTPS均200，前三文件逐项SHA匹配清单，收到的清单也与固定导出相等；页面摘要`a7dab8cd35282f12a915a286539230786c93f4724f228765259e153c1623c0d3`。

焦点BLE2-01为in_progress，原81.5%、20/169保持；任务书展示D-21真实Store/Auth局部实现与尚缺authorize/A/浏览器/IPC/Wi-Fi，未扣整步。仍仅同步13文件白名单，本地看板三文件也来自同一快照。原始下载与证明仅存私有update-2aa844a目录。此处补记不代表产品PR已合并或上机，也不冒记视觉/点击验收。

## D-21私有请求能力快照更新

北京时间2026-10-29 02:39复核：公开提交`da9c53fde7ecbdabc46ec541de70a21f1af03ee9`，固定私有来源`7f52333a420362112c98d66c58abd3bbabfaba2f`（PR61）。Pages构建1322293194为built，[运行40746958571](https://github.com/Viper-Boss/RustSimHub-progress/actions/runs/40746958571)completed/success。带独立缓存参数实际HTTPS下载四文件全部200，HTML/JSON/Markdown摘要逐项匹配清单，收到的清单与导出一致；HTML摘要`ae6d14a21dfa71d8e84e38260d8f0c2fe14870129279505af49b02695a953e36`。

焦点BLE2-01描述含本批302项离线结果及未接A/浏览器/IPC/Wi-Fi/账本边界，原81.5%、20/169和D余6不变。本地三文件同源，公开仍为13文件白名单。证明只在私有update-da9c53f目录，未声称产品合并/上机/视觉点击验收。本段补记不重做已通过的生产测试。
