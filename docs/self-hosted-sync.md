# 自托管多端同步

第一版为个人单工作区服务，不开放注册。独立 Rust 服务端和 SQLite 数据卷部署在自己的 VPS，Caddy 提供 HTTPS。桌面 DEB 不包含服务器、Docker、模型或解释器。服务端可读精简术语；传输使用 TLS，服务器磁盘加密和备份保管由部署者负责。

## VPS 准备和首次启动

建议至少 1 核、512 MiB 内存（1 GiB 更宽裕），为 SQLite/WAL、容器和独立备份留足空间。需要 Linux Docker Engine + Compose 插件、域名 A/AAAA 指向 VPS、可从公网访问 TCP 80/443；HTTP/3 可开放 UDP 443。不把服务 8080 端口发布到主机。数据卷必须在本机磁盘，不要使用 NFS 或两个实例共享 SQLite。

从源码根目录执行：

```bash
cd deploy
cp .env.example .env
# 编辑 .env：填真实域名、证书邮箱。
umask 077
openssl rand -hex 32 > sync-password.secret
# 仅将该应用同步密码输入客户端；它不是 VPS SSH 密码或模型 API Key。
docker compose --env-file .env config --quiet
docker compose build sync
docker compose run --rm -T --no-deps sync init < sync-password.secret
docker compose up -d
docker compose logs --tail 30 sync caddy
```

服务以 UID/GID 10001 运行，数据库文件 0600，数据目录 0700。新建命名卷继承镜像目录权限。若使用自行提供的绑定目录，预先设为该 UID/GID、0700；不使用 777。密码文件在主机保持 0600，只通过标准输入传给初始化或轮换命令，不挂载到运行中的服务。Caddy 证书可能需要等待 DNS 生效；查看日志时不要加入授权头或密码。服务日志仅输出启动状态和静态错误代码，不输出请求正文、令牌或原文。Caddy 未启用访问日志。

客户端在「偏好设置 → 多端同步」填写最终 HTTPS 地址、上述专用密码和设备名。先测试连接（不注册设备/不上传）；勾选范围说明后登录，点击立即同步。同步和后台调度默认关闭。后台仅随应用进程运行，每 60 秒检查，本机变更等待 2 秒后触发。失败有带抖动的指数退避，最大约 5 分钟；401 停止自动尝试，429 尊重 Retry-After。单轮最多 30 秒，分页进度和已发送操作持久化。

## 数据范围和冲突

同步术语、译名、领域、独立词义、出现时间/来源、显式熟悉程度；偏好只含外观、目标语言、解释风格。不上传原文、完整译文、示例、API Key、模型地址、画像、索引、本机隐私开关。关闭「记住专业术语」暂停记忆流，偏好流仍继续；重新开启先拉取错过的删除/全局清空，再提交旧数据。不会自动开启个性化或模型分析。

出现记录按稳定事件 ID 并集计数，复制同源数据库不会翻倍。术语同名但领域、词义或语言不同则分别保存。同一身份的显示字段沿用服务器首次接受的内容。熟悉程度和偏好使用服务器字段版本，系统时钟不决定胜负。同字段冲突显示本机和远端，用户选择；独立记录继续同步。已经发送但未收到响应的操作保留原 ID 和载荷重试，不把网络失败等同远端回滚。

删除术语同步到所有设备，并作废关联旧待提交记录。服务器擦除活动库和历史增量中的词条正文，只保留哈希键、生命周期、删除版本和收据摘要。确认删除后新的真实翻译可以重新记忆。清空所有设备需要单独确认，推进 memory generation，离线旧变更作废；偏好和登录保留。仅清空本机须先断开，同步重连可能从服务器重新取回数据。历史备份是独立副本，删除不会修改旧备份；需要按自己的保留规则删除备份及快照。

## 设备、密码和备份

客户端可列出、撤销设备；被撤销设备必须重新登录。轮换共享密码会撤销全部设备令牌但保留数据：

```bash
# 将新的长随机密码写入同一个 secret 文件后执行：
docker compose exec -T sync asa-sync-server rotate-password < sync-password.secret
docker compose exec sync asa-sync-server status
# 始终调用 SQLite 一致性备份命令，不直接 cp 活动数据库。
docker compose exec sync asa-sync-server backup /backups/before-upgrade.sqlite3
# 备份应另行复制到受保护的异地位置；命令拒绝覆盖同名备份。
```

恢复会生成新的 historyId，并撤销全部令牌。它不恢复旧 historyId 冒充从未中断的历史：

```bash
docker compose stop sync
docker compose run --rm --no-deps sync restore /backups/before-upgrade.sqlite3
docker compose up -d sync
```

服务锁防止运行中恢复，备份需通过 integrity_check 和 schema 校验。恢复后所有客户端先重新登录，确认新的历史基线：选择导入本机术语，或清除本机旧术语后只接收服务器。旧工作区/旧历史队列不会自动转移。模型配置与隐私授权保留。恢复用的备份文件仍独立保留，需要手动管理。

## 升级、限额和排错

升级前做一致性备份，拉取新版本源码后重新 build 和 up。服务 schema v1；不能将更新 schema 的数据库直接交给不兼容旧程序。客户端首次启用任意同步时，先在术语数据库旁生成 `memory.sqlite3.pre-sync-v1.sqlite3` 私有一致性备份，再原子迁移到 schema v2；本地 ID、计数、反馈和画像保留。未启用同步保持 schema v1。v2 重开不降级。降级客户端必须退出所有应用，保存当前数据副本，将 v1 备份恢复为原数据库路径，并移走对应 WAL/SHM 后再打开旧程序。v1 备份不含之后新增的数据，切勿覆盖它。

默认：20 个有效设备；每 IP 每分钟 5 次密码验证；同时 2 次 Argon2id 验证（64 MiB、3 次、并行度 1）；每设备每分钟 60 次业务请求；请求和实际响应最多 1 MiB；每批最多 100 操作；每页最多 500 条。Caddy 前置时登录 IP 限流按代理连接聚合，宁可保守也不信任客户端伪造的 X-Forwarded-For。更大设备规模需另行评估限额。

配额默认活动 SQLite 页 1 GiB，包含收据和元数据，由 `SYNC_STORAGE_BYTES` 调整。它不是物理文件/WAL/备份磁盘硬限额，应监控磁盘和备份占用。超配额新增事务回滚，仍可读/删/撤销。429 带 Retry-After；401 需登录；413 说明超过实际字节限制；字段冲突应在界面处理；history 改变必须重新确认；keyring 错误需解锁 Secret Service/凭据管理器，绝不回退明文。

Linux 客户端依赖系统 WebKitGTK 和凭据服务。X11 自动定位浮窗，Wayland 记住手动拖动位置。Windows、真实 VPS/公网证书和真实多屏需在相应环境另外验收，本地隔离测试不能替代这些环境。

## 使用随附的预构建镜像

交付目录提供 amd64 Docker 镜像归档和独立源码包。amd64 VPS 可以免去本机构建：

```bash
docker load -i asa-sync-server_0.3.2_amd64.docker.tar.gz
docker tag asa-sync-server:0.3.2 asa-sync-server:local
tar -xzf asa-sync-service_0.3.2_source.tar.gz
cd asa-sync-service-0.3.2/deploy
# 然后按「首次启动」配置 .env、生成密码并 init/up；跳过 compose build。
```

其他架构请从源码构建。Rust 编译建议至少 2 GiB 内存或配置 swap；运行服务的内存需求小得多。本机隔离 Compose 演练中，三次 Argon2id 密码轮换（包含 Compose exec 开销）约 0.169 秒/次，单次哈希阶段容器 peak 71,925,760 bytes；这只是当前测试机器的样本，不能当作 VPS 的延迟承诺。并发密码验证上限 2 的 HTTP 测试独立通过。
