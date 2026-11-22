# NKN 活跃安装签到

本服务按 [NKNGuard 的订阅续期方式](https://github.com/Viper-Boss/nknguard/blob/c32952d7586676fec10f71a30b22969cb24881f3/docs/USAGE_STATS.md) 为 RustSimHub 统计活跃安装。窗口和主题遵循 [RustSimHub 统计协议](../../docs/usage-stats/PROTOCOL.md)。控制台通过 NKN 的公开查询显示已经确认的订阅数量。

维护人员在设备上明确启用计时器后，系统会生成一个独立的随机统计签名密钥。每个窗口的订阅约每天续期一次，手续费固定为零。订阅公开统计公钥、主题和过期高度。服务不发送 SIM、短信、设备序列号或远程连接凭据。DNS、HTTPS 查询和提交固定走 wlan0。关闭服务后已有的订阅随区块自然到期。

在本目录用 Go 1.25.7 构建和测试。模块版本和校验值记录在 go.mod 与 go.sum。

```sh
go test -mod=readonly ./...
CGO_ENABLED=0 GOOS=linux GOARCH=arm64 go build -mod=readonly -trimpath -o rustsimhub-usage-checkin .
```

制作工厂系统树时，用与签名应用完全相同的归档、提交和归档 SHA-256 准备签到组件。先用 Go 1.25.7 下载 go.sum 锁定的依赖缓存，再执行离线构建。

```sh
python3 tools/nkn-usage-checkin/build.py \
  --archive "$archive" --revision "$revision" --source "$archive_sha256" \
  --output "$usage_checkin_artifacts"
```

给 `tools/compose-system-tree.py` 增加 `--usage-checkin-artifacts "$usage_checkin_artifacts"`。系统树会带上程序、复核的依赖许可证、服务与计时器。镜像不带统计身份，计时器保持关闭。所有者同意参与后，维护人员可在设备上启用计时器。

已安装的 RustSimHub 设备可由维护人员在校验 ARM64 产物后安装组件。

```sh
install -m 0755 rustsimhub-usage-checkin /usr/local/libexec/rustsimhub-usage-checkin
install -m 0644 rustsimhub-usage-checkin.service rustsimhub-usage-checkin.timer /etc/systemd/system/
systemctl daemon-reload
systemctl enable --now rustsimhub-usage-checkin.timer
```

停用时执行 `systemctl disable --now rustsimhub-usage-checkin.timer rustsimhub-usage-checkin.service`。保留服务状态目录即可在重新启用时沿用同一个统计身份。
