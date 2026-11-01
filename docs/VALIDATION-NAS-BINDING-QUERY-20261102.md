# NAS 绑定只读核对接口

本轮补齐主线管理接口：绑定或撤销请求回复丢失时，所有者可以查询实际当前映射，避免用重复绑定来“查询”而轮换 epoch、清理旧会话。它不是写操作历史回执，也不能自动判定某笔未知操作已成功。

## 实现

- 新增 v2 `legacy_binding_status {token,core_id}`；参数沿用128字节有限别名规则，禁止写参数/额外字段。
- 只经显式启用的兼容 provider 派发，普通认证运行模式拒绝。先验证最近5分钟登录的 Owner，再访问映射。
- 存在时返回原 binding 和独立 `principal_current`；核对当前 active NAS、kind、generation 和权限覆盖。已撤销或重新批准不继承旧映射。
- 不存在只返回 `known:false`；存储损坏返回错误，不伪装成已确认缺失。查询不创建身份/会话，不轮换 epoch、不延长有效期或作废现有会话。
- 保留现有NAS数据读取格式和许可，无新依赖、schema或网页交互；未修改成员的引擎、密码库或 Root 实现。

## 真实验证

固定产品提交 `21f4bbb306a7ed72be41610755279b027b50ac31`，完整源码归档 SHA-256 `07bc7e8a12c532c096df34cd14e455deb0caf0a05b4d99962ed6fa7e5b998c58`。

- 固定归档在本机 WSL 普通用户独立目录执行；复用已核验构建缓存，不称空缓存重建。
- `cargo +1.98.0 fmt --all -- --check` 通过。
- `cargo +1.98.0 test -p rustsimhub-wire -p rustsimhub-auth -p rustsimhub-core --all-features --locked -j 8`：362项通过（Auth105、Core库24/Core程序211、Wire22），0失败、0跳过。
- 新测试实际走 UnixStream 帧解析、Core handler/provider 和真实临时SQLite：关闭provider、无效会话、NAS会话拒绝；Owner查询不存在/绑定/撤销；重复读前后完整legacy.sqlite字节一致。
- 实际授权器测试另核对兼容bearer拒绝、查询后原会话仍可读取、撤销与同钥重新批准的代次隔离、损坏记录不能返回不存在。严格请求测试拒绝路径、超长别名、写权限/身份注入等。
- `cargo +1.98.0 clippy --workspace --all-targets --all-features --locked -j 8 -- -D warnings` 通过；原proc-macro-error2未来兼容提示仍保留，未当作已经修复。
- `cargo +1.98.0 build --release -p rustsimhub-core --no-default-features --locked --target aarch64-unknown-linux-gnu -j 8` 通过，ELF64/AArch64核对及产物独立复制/回读通过。Core SHA-256 `3dd88461bf6063d1d4bfd9834591a1c2c474cc8b8ecc82ec300c4542701c0fe4`。

权威私有验证记录 `nas-binding-query-validation-20261102-second.json` passed=true；源归档、日志和ARM产物只保存在私有目录。此前一次外层Shell退出码处理错误、一次WSL直接使用Windows worktree指针失败均保留；最终使用固定归档与无Shell状态拼接的执行器完成验证。

## 边界

没有部署、签名、设备执行、用户操作、新发短信/电话/数据或网页视觉验收。设备仍为此前应用70；成功NAS授权读取、完整旧NAS界面/控制/声音不因本接口计为通过。原P0–P7验收81.5%、行动20/169不变；主线代码约90%仍为粗估，后续继续集成。
