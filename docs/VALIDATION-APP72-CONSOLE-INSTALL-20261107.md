# 应用72控制台安装（2026-11-07）

## 来源
main `fed77b6dc599db177d9dc59464aed90e445e94b1`（含 PR217 控制台及 11-02 以来已合并的其他改动），源码归档 SHA-256 `87227efdbd5c527d8b0d428c01448a8e00783889a09127dee5936c8156cccdb0`。应用格式2、6载荷、schema6、epoch72，清单 SHA-256 `525fd9dd55a95a3317d4f08bca61295d90e10c1b39d01bca4b81b01f034a3ae4`，用设备已信任的既有开发签名，不是生产发行。

## 构建
- 第一次正式构建失败：`product_materials_package` 的 `actual_offline_encoder_binds_git_archive...` 要在 Git 仓库里跑 `git archive`，而正式构建用的是不含 .git 的冻结源码。main 上这个问题仍在，未改测试。
- 第二次构建给环境加同一提交的 `GIT_DIR` 后通过：12个ARM程序，Rust 1634通过/0失败/19专项忽略；音频 ARM QEMU 39 项通过。
- 候选离线验签和 6 项负向用例通过；恢复保护 12 场景夹具通过。

## 安装
NAS 的 USB 接口当前没有 10.55.0.1，原 USB 维护路径不可用；这次走局域网 SSH，复用同一台设备的固定主机钥匙和已批准维护钥匙。没有改 NAS。

只读预检后依次：上传回读、持写锁业务备份（SQLite 完整性通过、唯一 Owner、短信128）、admit-stage、单次启动、健康复核后 confirm。结果 confirmed72/floor72、无候选、未尝试。Root 三角色字节保持；setup-ap 契约一致，6 个静态材料逐字节相同。

复核：七个应用服务及 network-agent、status-led 为 active；转发 PID 0；蜂窝数据断开；音频 7/7；恢复 timer 停止、预算未用；可信 HTTPS 八个资源（含 console.js、console.css）与源码逐字节一致；短信 128 条前后保持，受保护表一致。

在 Core 命名空间内确认 `system_status` 的来源文件可读（/proc、os-release、device-tree、thermal）。这是只读探测，不是接口调用结果。

本轮没有重启棒子/NAS、切 Wi-Fi、启普通 APN、拨号、发短信/推送、刷分区或碰基带。

## 未验收
- 登录后的控制台页面未在真机浏览器中看过，`system_status` 未经真实登录调用。需要用户用所有者账号打开确认。
- 真实故障回退、冷启动未测。
- `tools/test-web-app-assembly.mjs`、`tools/test-web-setup-browser.mjs` 未运行（本机无 Playwright）。
- 本次带上了 11-02 以来 main 的全部已合并改动，只验证了上述范围，不代表这些功能已逐项实机验收。

## 同日追加：应用73（各模块界面和手机版）
main `c5b52558297325da2f7e17fd58136c48dd80e937`（PR219），源码归档 SHA-256 `ab99e45b0951108d0a2eedeee64dff3343851b4b9108bba950f04151b3d2d0d9`，epoch73，清单 SHA-256 `10749bc407f4e2f650bf8857a4d84e3a8db01b2424bb73bcf0f62dbd13a7c819`。构建同样需要带 `GIT_DIR`：12个ARM程序、Rust 1634通过/0失败/19专项忽略，音频 QEMU 39，离线验签及6项负向、恢复夹具12场景通过。

同一流程经局域网 SSH 安装：confirmed73/floor73、无候选。七服务及 network-agent、status-led active，Core、Edge 未重启，转发 PID 0，蜂窝数据断开，音频 7/7，八个资源逐字节一致，短信 128 条保持，Root 三角色字节保持。

未验收：登录后的页面、`system_status` 真实调用、手机和电脑实际观感。用户反馈的“数据不显示”原因未确认。（后续更新：见下方应用74一节。）

## 同日追加：应用74（登录修复和最新记录）
main `55f9783910590b7b6ca4612af2ef9ccbd6c7c305`（PR220），源码归档 SHA-256 `7269d0a775e7b1374274ec9c5e04d9daf4ce1d7730af0fff44709d66f7c0da3f`，epoch74，清单 SHA-256 `77559ba0122e234578e3df60b259cba2052d1e6bd18d2392e018128a8d0d9648`。官方构建 1638 Rust 通过、0 失败、19 专项忽略，音频 39，候选校验 6 个反向用例，恢复脚本 12 场景。

同一流程经局域网 SSH 安装：confirmed74/floor74、无候选、恢复定时器已停且未使用。七服务及 network-agent、status-led active，Core、Edge 重启 0 次，转发 PID 0，蜂窝数据断开，音频 7/7，备份和确认前后短信 128 行，唯一所有者保持。页面 200，含新登录入口。

“数据不显示”的原因已查明：所有者 10-23 用浏览器钥匙认领，账号密码登录 10-28 才加入；浏览器里曾有一次账号密码提交，旧代码把当前钥匙换掉且失败不换回，因此无法登录。经用户批准，原钥匙已在该浏览器放回（只改浏览器本地存储）；应用74起失败不再替换钥匙。

真机浏览器已验证：新登录按钮登录成功；`system_status`、`inbox_recent`、`calls_recent` 真实返回；首页显示最新短信和验证码，总数短信 127、通话 20。

发现未修：
- 列表按到达设备的顺序排，和短信自带时间不完全一致，首页“最近短信”第三条早于列表里的其他记录。
- 真机短信时间未被页面解析，原样显示，带 `T`。
- 部分已结束通话显示“状态核对中”，原因未查。
- 短信 127 与数据库 128 行差 1，疑为回收站 1 条，未核实。

未验收：其他页面逐项、手机实际观感、用户验收。`test-web-app-assembly`、`test-web-setup-browser`、`test-web-recovery-session` 本机未运行；GitHub CI 因账单问题未启动。
