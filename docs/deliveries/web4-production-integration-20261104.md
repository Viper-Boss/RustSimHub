# WEB4-INTEGRATE：负责人阶段一交付与合并记录

日期：2026-11-04（Asia/Shanghai）。用户指定窗口8由负责人接手，6/7及原窗口3继续各自开发。本报告是**阶段一，不是窗口8十四步全部完成**。未访问设备/NAS，未生产签名或安装。

## 已实际合入main

用户本轮授权直接合并已完成PR；逐项核对固定HEAD/范围并复核后正常merge，没有管理员绕过、直推main或更改保护规则。

| PR | 固定HEAD | 实际merge commit |
|---|---|---|
| 205 发行profile | `c3a67120cb91f8c6095579a89764466a101d9433` | `93046b7232e96d10e37380b88fe9c55930f9015c` |
| 202 AP权威回执 | `ff848793f79981b5bdd796002634f12db65e7d55` | `4d779d502444e0323d1412d1ca6780d704b65f88` |
| 204 AP网页流程（含202） | `b671aaf1828583aa65a16329c31538e53096f229` | `6518f74beacf9b9fdaebc9bfd02979031b23f6e2` |
| 203 恢复actor控制顺序 | `56a1055d583c1f7f7d7d21f1b081f07a54dd3e85` | `01028d98b03c2e8b2b4f66a4ed0e216d2d8eac47` |
| 207 六VM依赖/主页装配 | `6099846b54e8853d2759fd49d9961ae440bf5e97` | `f128dfea78ece40fa5bc82ccbc2fd34d578ecaeb` |

202/204、197/200的包含关系按历史保留；没有重复复制依赖代码。其余195～200/201/206与草稿未因本报告被宣称已合并。206是健康第一阶段，不是最终自动确认。当前GitHub CI的failure和实际测试失败分开：已读annotation明确付款/消费限制未启动，相关jobs实际0步骤；不称云端CI通过。

## 本次新增代码

1. `tools/product-integration/inputs.json`冻结完整输入SHA、依赖/实际包含关系和明确待交付项；`verify.py`用真实Git对象/历史、非浅仓库、固定组合HEAD及工作树状态检查，不按PR标题/数量认完成。报告不供应产品依赖；不联网/写Git/执行候选/部署，所有授权/健康/安装字段保持false。严格模式缺交付或未组装退出3，非法输入退出2。工作树status不代替原构建器的完整归档字节校验。
2. 修复207交出的真实生产UI缺口：原 `app.js` logout先reset NKN、后清token，导致只读按钮仍显示可点。先追加真实app回归，负例提交 `0a3a1e842ba96e828e9f0c338d655caffe81efe8` 实际退出1，消息 `NKN read must be disabled after the actual app logout`。修复后先完成退出再reset；重新登录也先清旧token/lease再刷新面板，登录失败不留下旧按钮状态。原权限/epoch/RPC检查保持，未增加自动读/启停。
3. 强化实际app测试和真实浏览器断言：logout/迟到回包/重登录失败后NKN全部操作按钮禁用，真实Chrome不再条件性接受KNOWN PRODUCT GAP。管理登录仍独立可用。
4. 两个浏览器测试runner用 `fileURLToPath` 转换脚本URL；Windows不能把 `/C:/...` 当作本地Python文件路径。该修正已在本机Windows Chrome实际运行，不依赖放宽CSP。

## 精确验证来源与结果

| 来源/检查 | 实际结果 |
|---|---|
| `ac5afcc6eed8bb7a3f82146dbb5e7a2835cefa54`，main205＋AP204/202＋恢复203 | Core97+213、Edge41、setup-ap180，共531通过/0失败/5原专项忽略；三包all-features/all-targets严格Clippy通过，AP三入口含67组实际控制器夹具通过 |
| `0e5d8749e4e01e917f2991e569bd8d5d214481c7`，初版输入工具 | 12项Linux Git/CLI测试、真实固定输入检查通过；严格模式退出3，准确列出当时四项待交付。与ac5的crates/firmware/native字节无差异 |
| `71b268cb185f39b0d98bdc1c3b83b9236abb7754`，本次最终产品/测试代码 | 31个适合Node的网页入口全部通过，含六个VM、真实模块/别名、六正/负对照、退出与重登录回归；Edge41及严格检查重新通过，输入工具12项通过 |
| 同一71b，本机Windows Chrome | 真实HTML/ESM/原CSP/IndexedDB/合成双向Ed25519登录，320/390/1280三宽首页与退出禁用、缺模块负例通过；AP浏览器11场景通过，两标签页/重开/取消/恢复不明/错密码等保持 |
| 同一71b，真实输入lock/CLI | 已纳入207；严格模式退出3，待交付为health-confirm、product-install、product-data；另有已定位但未组装的旧PR，不假报source_assembly_complete |
| `30c6c2c266016505e16463c8072fc3dba61ad5ae` | 正常同步207实际merge的历史提交；与受测71b全部文件无差异，后续仅本报告/进度文档 |

531与后续重复的Edge41不相加，原生浏览器与Node夹具不当设备/RF/Core→特权Root联调。31个Node入口显式排除两个浏览器launcher（已另用本机Chrome执行）及 `test-web-recovery-session.mjs`（浏览器/WASM/native peer专项本轮未运行）。本轮没有新ARM构建或全工作区回归；205原固定产品的13/16 ARM构建及35/18/14等结果见其原报告，不冒充本次新组合结果。

本机Chrome使用私有venv和临时浏览器profile、既有Chrome程序；未修改用户已有页签、资料、全局Python或生产依赖。原始日志/截图、合成密钥、工具运行JSON仅私有证据目录，不提交。

主要复现命令：

```sh
cargo +1.98.0 test -p rustsimhub-core -p rustsimhub-edge -p rustsimhub-setup-ap --all-features --locked --offline
cargo +1.98.0 clippy -p rustsimhub-core -p rustsimhub-edge -p rustsimhub-setup-ap --all-features --all-targets --locked --offline -- -D warnings
python3 -m unittest discover -s tools/product-integration -p 'test_*.py' -v
python3 tools/product-integration/verify.py --repository /private/full-checkout --integration FULL_COMMIT_SHA --require-complete
node tools/test-web-test-center.mjs
node tools/test-web-app-imports.mjs
node tools/test-web-app-imports-negative.mjs
# 其余Node入口逐一运行；浏览器/WASM入口单列，不盲目glob宣称通过。
node tools/test-web-app-assembly.mjs
node tools/test-web-setup-browser.mjs
```

原始失败保留：首次WSL验证缺工具链PATH，cargo未启动（不是代码失败）；补原工具链环境后同一ac5通过。首次Windows受限临时目录无法git init/清理，未进入新工具测试；随后Linux真实Git用例12通过，未弱化门禁。原207已记录的历史失败仍保留，本次生产UI负例与修复另绑上述SHA，不追认旧代码已通过。

## 十四步当前进展与下一项

1的来源图/检查已实现，完整PR状态/最终依赖还需随交付更新；2已接纳上述五PR，旧输入仍待审；3独立工具已实际完成。4～12的完整生产安装/数据/自动健康调用链**仍未完成**，必须消费窗口3第二阶段和6/7真实交付，不以当前检查工具替代。13仅上述固定专项组合，不是最终全仓/ARM/设备验证；14先交本阶段，后续再交共享入口与发行组合。

主线继续审核其他已交PR，依赖交付后正常整合并补真实CLI/有限维护端到端。窗口6/7、健康窗口和原成员文件不被本阶段抢写。未签名、分配epoch、安装、变更真实NSS/服务、拨号/短信/公网NKN/TURN或关机调试。PMIC搁置保持，整体验收81.5%、20/169、加权10.6%不变；本阶段不是100%代码或窗口8完成声明。
