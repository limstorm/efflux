# 流光 · Efflux

> 把生活的点滴，交给时间慢慢流过。

一个私密的生活记录站点：写下主题、照片、视频、文字、背景音乐与标签，绑上一个时间。
主页不是一张网格列表，而是一条**会自己流动的光河** —— 记忆像灯影一样顺流而过，
你可以随时伸手拦下某一段，翻开它，再放回去。

当前版本 **v1.0.0** · 单用户私密使用 · 自己部署在自己的机器上

<p align="center">
  <img src="docs/screens/river.png" width="24%" alt="光河主页" />
  <img src="docs/screens/moment.png" width="24%" alt="翻开一段记忆" />
  <img src="docs/screens/gallery.png" width="24%" alt="图集" />
  <img src="docs/screens/lightbox.png" width="24%" alt="看大图" />
</p>

---

## 快速开始

需要 Docker（含 compose 插件）：

```bash
git clone https://gitee.com/limstorm/efflux.git
cd efflux
cp .env.example .env      # 至少改掉 POSTGRES_PASSWORD；想现在就设访问口令就填 EFFLUX_ACCESS_CODE
./deploy/docker.sh up
```

浏览器打开 <http://localhost:8080> 就能看见光河。首次构建会久一点（编译 Rust + 下载可选的 CLIP 模型，十几分钟）。

不想用 Docker、想直接跑在自己的机器上，见下面「[部署](#部署)」的方式二。

---

## 特性

### 看

- **光河主页** — 记忆按时间自发从右往左缓缓流过眼前，可拖动、滚轮、键盘左右键自由回溯；
  停留几秒后时间重新开始流动；可随时暂停（空格）。
- **巡演（演示）** — 右下角那枚放映图标里可以选「顺序演示」或「随机演示」：
  镜头自己摇过去、翻开、退回光河，像一场只放给自己的电影；右下角一枚叉号随手退出（Esc 也行）。
- **图集** — 所有画面按时间铺开，可按标签、关键词筛；点开看大图，左右翻页。
- **找相似的画面** — 开了自动识别（见「图片自动打标签」）之后，每张照片会留下一枚 CLIP 向量，
  在图集里就能「找相似」，翻出同一场景的其它画面。
- **点亮地图** — 去过的省与市按记忆条数着色；点亮起来的省下钻到市，再点市就去看那里的记忆。
  南海诸岛收在右下角的小框里，不占主图、比例也不走样。

<p align="center">
  <img src="docs/点亮地图-省级.png" width="46%" alt="点亮地图：省级" />
  <img src="docs/点亮地图-下钻.png" width="46%" alt="点亮地图：下钻到市" />
</p>

### 记

- **几分钟就能记完** — 右上角的加号进入记录页：丢进照片/视频（或直接拖拽），写几句话，按下保存。
  时间默认「此刻」，不必填写任何设置；标签、自定义时间都是可选的。
- **一次可以记下很多画面** — 照片与视频支持多选，按住缩略图（触屏约 0.25 秒）能拖动排序，
  排在最前的会成为时间轴封面；多张照片在详情页会拼成相册的一页。
  单击缩略图可以直接看大图，在大图里左右翻看这一段的所有画面，也能就地移除。
- **背景音乐与氛围音** — 记录页里「画面」与「背景音乐」是两块：没有音频文件也不打紧，
  「细雨 / 夜虫 / 炉火 / 低频」这四段氛围音是**现场算出来的**（Web Audio 合成，零文件），
  点一下就能听见，不占空间、不进备份，分享出去对方也听得到。
  默认是**安静**——不替谁先放上一段声音；想配乐就点一下，或者传一首自己的曲子进去
  （氛围音与文件二选一）。配上的那段，打开记忆时会自动播放。
- **城市自己认出来** — 照片带着定位时，保存的那一刻就去认一座最近的城市，点亮地图用的就是它。

### 找回

- **按关键词 / 时间 / 标签找回** — 关键词的射程覆盖标题、正文、地点与城市；
  时间可选「今年 / 本月 / 最近一周」，也可以自己划一段区间。
- **标签会自己收拾** — 输入时补全已有标签；没人再用的标签过一阵子自己消失。
- **分享一小段给朋友** — 一段记忆一个凭证：链接发给没登录的人，也只看得见这一段。

### 收好

- **备份与恢复** — 整站（数据库 + 全部照片视频音乐）打成一个 zip，可以下载，也可以再吃回去。
  自动备份能设成每天 / 每 3 天 / 每周 / 每两周 / 每月，存到别的盘上去；
  包走增量链，只留最新一份，换目录时会把已有的包一起搬走。
- **十套纸面配色** — 宣纸、晨雾、暮山、苔痕、旧书、红叶、樱、深海，外加两套夜色（夜航、星空），
  在「设置」里挑一套顺眼的。
- **访问口令** — 一句只有家人知道的话。存的是 PBKDF2 哈希，可以随时在网页上改；
  设上之后，未登录连照片的 URL 都取不到（分享链接除外）。
- **装进手机主屏幕** — 自带 PWA 清单与图标。从主屏幕打开是全屏，没有地址栏，光河铺满整个屏幕。
- **（可选）图片自动打标签** — 打开 `EFFLUX_TAGGER=1` 并放好 CLIP 模型，上传的图片会被自动认一认
  （「海边」「猫」「食物」…），找相似用的向量也在这条流水线上留下。

---

## 技术栈

| 层 | 选型 | 说明 |
|---|---|---|
| 前端 | Vue 3 + TypeScript + Vite + Tailwind CSS 4 | 单页应用，自适应，无重型 UI 框架；地图用 ECharts |
| 后端 | Rust + Axum + SQLx | 异步、类型安全、低延迟 |
| 数据库 | PostgreSQL 16+ | 标签多对多、`pg_trgm` 加速模糊搜索 |
| 部署 | Docker Compose 或单进程 | Docker：`web`（Nginx）+ `backend` + `db` 三个容器；单机：一个后端进程顺带发前端 |

Docker 部署时媒体文件由 **Nginx 直接服务**（挂载同一个数据卷），不经过应用层，大图大视频的延迟更低；
本地部署时同一个后端进程把页面、媒体、API 一起发出去，不必另装 Nginx。

## 环境要求

| 部署方式 | 需要 |
|---|---|
| Docker Compose | Docker 24+（含 compose 插件） |
| 本地部署 | Rust 工具链（<https://rustup.rs>）、Node.js 20+、PostgreSQL 16+ |
| 本地开发 | 同上（不必装 Nginx） |
| 运行时 | 现代浏览器：iOS Safari 15+ / Chrome 90+ / Edge 90+ / Firefox 90+；用到了 Web Audio、`backdrop-filter`、CSS 变量与 `color-mix()` |

---

## 部署

两种方式挑一种即可，数据都留在自己的机器上。

### 方式一：Docker Compose（推荐）

需要 Docker（含 compose 插件）。

```bash
git clone https://gitee.com/limstorm/efflux.git
cd efflux

cp .env.example .env      # 至少改掉 POSTGRES_PASSWORD；想现在就设口令就填 EFFLUX_ACCESS_CODE
./deploy/docker.sh up
```

浏览器打开 <http://localhost:8080>。首次构建会久一点（编译 Rust + 下载 CLIP 模型，十几分钟）。

```bash
./deploy/docker.sh status     # 看容器状态与访问地址
./deploy/docker.sh logs       # 跟踪日志
./deploy/docker.sh restart    # 重启
./deploy/docker.sh down       # 停止（数据留着）
./deploy/docker.sh upgrade    # 拉代码 → 重新构建 → 重启
./deploy/docker.sh destroy    # 停止并删掉数据（会要一次确认）
```

数据放在两个命名卷里：`db_data`（数据库）、`media_data`（照片/视频/音乐）。
容器随便删，卷不动，记忆就还在。

### 方式二：本地部署（不用 Docker）

需要：Rust 工具链（<https://rustup.rs>）、Node.js 20+、PostgreSQL 16+。

```bash
sudo -u postgres psql -c "CREATE USER efflux WITH PASSWORD 'efflux_local_password' SUPERUSER;"
sudo -u postgres psql -c "CREATE DATABASE efflux OWNER efflux;"

./deploy/local.sh install     # 检查依赖 → 确认数据库 → 构建前后端 → 启动
```

脚本只跑**一个后端进程**：设上 `EFFLUX_STATIC_DIR` 之后，前端页面、媒体文件、API 都由它发出去
（配置见下面的「配置项」）。数据默认落在仓库里的 `.data/`。

```bash
./deploy/local.sh status      # 看状态与访问地址
./deploy/local.sh logs        # 跟踪日志
./deploy/local.sh restart     # 重启
./deploy/local.sh down        # 停止
./deploy/local.sh upgrade     # 拉代码 → 重新构建 → 重启
```

`install` 跑完会在末尾打印一份 **systemd 单元模板**，想让它在开机时自己起来，
把那段存成 `/etc/systemd/system/efflux.service`，然后：

```bash
sudo systemctl daemon-reload && sudo systemctl enable --now efflux
```

数据库连接串、端口、数据目录都可以在 `.env` 里改，或者临时用环境变量覆盖：

```bash
EFFLUX_PORT=9000 ./deploy/local.sh up
DATABASE_URL=postgres://user:pass@10.0.0.5:5432/efflux ./deploy/local.sh install
```

调试时嫌 release 编得慢，可以 `EFFLUX_BUILD_PROFILE=debug ./deploy/local.sh install`。

---

## 如何升级

**升级前先备份一份**，这是唯一要紧的事（在站点「设置 → 备份」里点一下导出，或者照下面「数据在哪」自己 dump）。

```bash
# Docker
cd efflux && ./deploy/docker.sh upgrade

# 本地
cd efflux && ./deploy/local.sh upgrade
```

两个脚本做的事一样：`git pull --ff-only` → 重新构建 → 重启，**数据不动**。
数据库迁移在后端启动时自动执行，所以升级后第一次启动会稍慢一点。

手动升级（不想用脚本时）：

```bash
git pull --ff-only

# Docker：重新构建并启动
docker compose up -d --build

# 本地：重新构建前端与后端，再重启进程
(cd frontend && npm ci && npm run build)
(cd backend && cargo build --release)
./deploy/local.sh restart
```

关于回退：迁移是单向前进的，升级后想退回旧版本，旧版可能不认识新表，
所以回退前请用升级前的备份恢复，而不是只把代码切回去。

### 数据在哪

| 部署方式 | 数据库 | 照片 / 视频 / 音乐 |
|---|---|---|
| Docker | 命名卷 `db_data` | 命名卷 `media_data` |
| 本地 | PostgreSQL 里的 `efflux` 库 | `EFFLUX_DATA_DIR`（默认 `<仓库>/.data`） |

整套搬走 = 换台机器照样能起来：把数据库 dump 出来、把媒体目录拷过去即可。

```bash
# 导出数据库
docker compose exec db pg_dump -U efflux efflux > efflux.sql     # Docker
pg_dump -U efflux efflux > efflux.sql                            # 本地

# 媒体目录（Docker 里就是 media_data 卷，可以捞出到一个目录）
docker run --rm -v efflux_media_data:/data -v "$PWD":/out alpine tar cf /out/media.tar -C /data .
```

更省事的做法是用站点自带的备份：它把数据库与媒体一起打进一个 zip，在「设置 → 备份」里可以下载、
也可以导入回去。导入是按修改时间比的 —— 包里那份更新才覆盖，所以拿一份旧备份还原，
不会把本地新写的内容冲掉，搬家和找回误删都合适。

---

## 放到公网之前

### 1. 设置访问口令（必做）

默认状态是**完全不设防**的——知道网址的人都能查看、上传、删除。设置口令后，
未登录的请求（包括媒体文件）都会被挡住。

```bash
# .env
EFFLUX_ACCESS_CODE=一句只有家人知道的话
EFFLUX_COOKIE_SECURE=true   # 走 HTTPS 时必须打开
EFFLUX_SESSION_DAYS=30
```

家人打开站点会看到登录页，输入一次口令，之后 30 天内不用再输。

口令**存的是 PBKDF2 哈希，可以在网页上随时修改**：主页右上角齿轮 → 设置。
改完立刻生效、不用重启服务，其他设备上的登录会全部失效（当前这台会自动换一张新通行证）。
环境变量 `EFFLUX_ACCESS_CODE` 只是首次启动时的初始值，之后改环境变量不再起作用。

### 2. 上 HTTPS（必做）

口令和会话 Cookie 走明文 HTTP 等于没设。两种最省事的做法：

**A. Caddy 自动申请证书**（域名已解析到服务器时最省事）

```caddyfile
efflux.example.com {
    reverse_proxy localhost:8080
}
```

**B. Cloudflare Tunnel**（家里 NAS / 树莓派，完全不暴露公网 IP）

```bash
cloudflared tunnel --url http://localhost:8080
```

### 3. 已经内置的加固

| 措施 | 说明 |
|---|---|
| 登录延迟 | 口令不对时后端压 500ms 再回（给暴力尝试减速） |
| 登录限流 | 12 次/分钟/IP，另攒 5 次 —— **后端自带**，两种部署都生效 |
| API 限流 | 30 请求/秒/IP，另攒 60 次 —— **后端自带**；Docker 部署里 Nginx 前面还有同样的一道，等于多一层保险 |
| 并发连接上限 | 每 IP 30 条 —— 只有 Docker 部署里有（Nginx `limit_conn`） |
| 会话 Cookie | `HttpOnly` + `SameSite=Lax`，走 HTTPS 时加 `Secure` |
| 常量时间比较 | 口令与令牌签名都用常数时间比对，避免时序侧信道 |
| 媒体文件鉴权 | 未登录取不到照片：Docker 里由 Nginx `auth_request` 挡，本地部署里由后端同一道中间件挡 |
| 上传白名单 | 只接受已知的图片/视频/音频 MIME；图片与音频有大小上限，视频不设上限 |
| 响应头 | 隐藏 Nginx 版本、`X-Content-Type-Options`、`X-Frame-Options` 等（Docker 部署） |

限流按客户端地址分桶：前面站着 Nginx / Caddy / Tunnel（对端是本机或内网）时，按 `X-Forwarded-For`
里的头一条算；直接暴露在公网时那个头谁都能编，只认 TCP 的对端地址。嫌紧了可以 `EFFLUX_RATE_LIMIT=off` 关掉。

不管哪种部署，都别把没上 HTTPS 的端口直接开到公网 —— 放在内网、或用下面的 Tailscale。

### 4. 更彻底：干脆不暴露公网

如果连"公网上有个入口"都不想要，用 Tailscale 把家人的设备组成一个私有网络：

```bash
tailscale up
tailscale serve https / http://localhost:8080
```

家人装上 Tailscale 并加入网络后，通过内网地址访问即可；服务对公网完全不可见，
也就不存在被扫描、被爆破的问题。

---

## 键盘与手势

| 在哪 | 操作 | 作用 |
|---|---|---|
| 光河 | 拖动 / 滚轮 / `←` `→` | 前后回溯时间 |
| 光河 | `Home` / `End` | 跳到最早 / 此刻 |
| 光河 | `空格` | 暂停或继续流动 |
| 光河 | 点一下画面 | 翻开这一段记忆 |
| 记忆页 | `Esc` | 退回光河 |
| 大图 | 左右滑动 / 点两侧箭头 / `←` `→` | 翻看这一段的所有画面 |
| 大图 | `Esc` | 关掉大图 |
| 记录页 | 长按缩略图（触屏约 0.25 秒）/ 直接拖（鼠标） | 拖动排序，第一张即时间轴封面 |
| 记录页 | 单击缩略图 | 看大图（大图里可左右翻看、也可移除某一段） |
| 巡演 | `Esc` | 退出演示 |

---

## 配置项

后端全部通过环境变量配置（启动时校验，缺失即退出）：

| 变量 | 默认值 | 说明 |
|---|---|---|
| `DATABASE_URL` | —（必需） | PostgreSQL 连接串 |
| `EFFLUX_PORT` | `8080` | 监听端口 |
| `EFFLUX_DATA_DIR` | `./data` | 媒体文件存放目录（本地部署脚本默认给 `.data`） |
| `EFFLUX_STATIC_DIR` | 未设置 | 打包好的前端目录（如 `frontend/dist`）。设上之后后端顺带托管网页与 SPA 回退，本地部署就靠它 |
| `EFFLUX_MAX_UPLOAD_MB` | `512` | 图片 / 音频的单个文件大小上限；**视频不设上限** |
| `EFFLUX_DB_MAX_CONNECTIONS` | `16` | 连接池上限 |
| `EFFLUX_CORS_ORIGINS` | `http://localhost:5173` | 允许的跨域来源（逗号分隔），只在前后端分开跑的开发模式下用得上 |
| `EFFLUX_ACCESS_CODE` | 未设置 | 家庭访问口令，**只用于首次初始化**；之后可在站点「设置」里随时改。留空 = 不做鉴权（仅本机开发） |
| `EFFLUX_SESSION_DAYS` | `30` | 登录状态保持天数 |
| `EFFLUX_COOKIE_SECURE` | `false` | 会话 Cookie 是否只在 HTTPS 下发送（公网部署设为 `true`） |
| `EFFLUX_RATE_LIMIT` | `on` | 后端自带的限流（登录 12 次/分钟、其余 API 30 次/秒，按客户端地址）。设 `off` 关掉 |
| `EFFLUX_TAGGER` | `false` | 是否开启图片自动打标签与相似照片（需要模型文件，见下） |
| `EFFLUX_MODEL_DIR` | `./models` | CLIP 模型目录 |
| `EFFLUX_LOG_JSON` | 未设置 | 设置后输出 JSON 日志 |
| `RUST_LOG` | `info` | 日志级别 |

### 自动识别（可选）

`EFFLUX_TAGGER=1` 时，`EFFLUX_MODEL_DIR` 里要有这三样（量化版 CLIP，加起来约 156MB）：

```
models/
├── vision_model.onnx
├── text_model.onnx
└── tokenizer.json
```

Docker 部署在构建镜像时已经自动下载好了（走 ModelScope），不用管。
本地部署想要的话，自己拉一份：

```bash
mkdir -p backend/models && cd backend/models
BASE="https://www.modelscope.cn/api/v1/models/Xenova/clip-vit-base-patch32/repo?Revision=master&FilePath="
curl -fL -o vision_model.onnx "${BASE}onnx/vision_model_quantized.onnx"
curl -fL -o text_model.onnx   "${BASE}onnx/text_model_quantized.onnx"
curl -fL -o tokenizer.json    "${BASE}tokenizer.json"
```

然后在 `.env` 里加 `EFFLUX_TAGGER=1`、`EFFLUX_MODEL_DIR=backend/models` 再重启。
模型有问题不会拦住服务启动，只是这块功能不生效。

---

## API

统一前缀 `/api`。错误响应格式：`{"error":{"code":"...","message":"..."}}`。
除了 `/health`、`/ready`、`/auth/*` 与分享接口，其余都要带会话（设了口令时）。

| 方法 | 路径 | 说明 |
|---|---|---|
| `GET` | `/moments` | 列表。支持 `q`（标题/正文/地点/城市模糊）、`tags`（逗号分隔）、`from`/`to`（时间范围）、`order`、`limit`、`offset` |
| `POST` | `/moments` | 创建点滴 |
| `GET` | `/moments/{id}` | 详情（含媒体、背景音乐与氛围音） |
| `PATCH` | `/moments/{id}` | 更新 |
| `DELETE` | `/moments/{id}` | 删除 |
| `GET` | `/moments/stats` | 总数、时间跨度、标签数 |
| `POST` `DELETE` | `/moments/{id}/share` | 生成 / 作废这一段记忆的分享凭证 |
| `GET` | `/shared/{token}` | 分享出去的那一段（无需登录） |
| `GET` | `/shared/{token}/media/{id}` | 分享页里的媒体（无需登录） |
| `GET` | `/gallery` | 图集：所有画面（`tags`、`q`、`limit`、`offset`） |
| `GET` | `/gallery/tags` | 图集顶部的标签统计 |
| `GET` | `/gallery/{id}/similar` | 找相似（按 CLIP 向量算余弦） |
| `GET` | `/map/provinces` | 点亮的省：记忆数、城市数、最近一次 |
| `GET` | `/map/cities` | 某省的市（`province=330000`） |
| `GET` | `/geo/city` | 经纬度 → 城市名（`latitude`、`longitude`） |
| `POST` | `/media/upload/{kind}` | 上传媒体，`kind` = `image` \| `video` \| `audio`；multipart 字段 `file`、`thumb`、`width`、`height`、`durationMs` |
| `DELETE` | `/media/{id}` | 删除尚未使用的媒体 |
| `GET` | `/tags` | 标签及使用次数 |
| `GET` `POST` | `/backup` | 备份列表 / 立刻备份一份 |
| `GET` | `/backup/{filename}` | 下载备份包 |
| `POST` | `/backup/restore` | 导入一个备份包 |
| `GET` `PATCH` | `/backup/settings` | 自动备份的周期与存放目录 |
| `GET` | `/auth/status` | 这个部署要不要口令（公开） |
| `POST` | `/auth/login` `/auth/logout` | 登录 / 退出 |
| `GET` | `/auth/session` | 当前会话状态 |
| `POST` | `/auth/change-code` | 改访问口令 |
| `GET` | `/auth/check` | 给 Nginx `auth_request` 用的探针 |
| `GET` | `/health` `/ready` | 存活 / 就绪检查 |
| `GET` | `/files/*` | 媒体文件（端口直出，带鉴权） |

创建点滴的流程：先 `POST /media/upload/{kind}` 拿到媒体 id，再在 `POST /moments` 的 `media`
字段里按顺序关联。未关联的媒体会在 24 小时后自动回收。

---

## 本地开发

需要本机有 PostgreSQL（本地部署的那套库直接用就行）。

```bash
# 1. 准备数据库（已经建过就跳过）
sudo -u postgres psql -c "CREATE USER efflux WITH PASSWORD 'efflux_local_password' SUPERUSER;"
sudo -u postgres psql -c "CREATE DATABASE efflux OWNER efflux;"

# 2. 启动后端（http://localhost:8080）
cd backend
DATABASE_URL="postgres://efflux:efflux_local_password@127.0.0.1:5432/efflux" \
EFFLUX_DATA_DIR=../.data \
cargo run

# 3. 启动前端（http://localhost:5173，已配置代理到 8080）
cd frontend
npm install
npm run dev
```

开发模式下前后端分开跑，`/api` 与 `/files` 都由 Vite 代理到后端，改前端有热更新。
想换个后端地址：`VITE_PROXY_TARGET=http://127.0.0.1:9000 npm run dev`。

前端的常用命令：

```bash
npm run dev          # 开发服务器
npm run build        # 类型检查 + 打包到 dist/
npm run typecheck    # 只做全量类型检查
npm run preview      # 本地预览打包产物
```

### 导入演示数据

`seed/` 目录里有几张示例照片和一段导入脚本（需要 `curl` 与 `python3`）：

```bash
python3 seed/import.py http://localhost:8080
```

---

## 目录结构

```
efflux/
├── backend/                 # Rust 服务
│   ├── migrations/          # SQL 迁移（启动时自动执行）
│   ├── models/              # CLIP 模型（可选，见「自动识别」）
│   └── src/
│       ├── moments/         # 点滴：模型 / 仓储 / 服务 / 路由处理
│       ├── media/           # 媒体：上传、流式写盘、类型白名单
│       ├── map/             # 点亮地图：省市映射表
│       ├── geo.rs           # 经纬度 → 城市
│       ├── gallery.rs       # 图集与找相似
│       ├── backup.rs        # 备份、恢复与定时器
│       ├── tagger/          # CLIP：自动标签与向量
│       ├── auth/            # 口令、会话、鉴权中间件
│       ├── ratelimit.rs     # 令牌桶限流（登录 + API）
│       ├── settings/        # 站点设置（存库）
│       ├── config.rs        # 环境变量集中校验
│       ├── error.rs         # 领域错误层级 + 统一错误响应
│       └── router.rs        # 路由装配与中间件
├── frontend/                # Vue 应用
│   ├── public/              # 图标、PWA 清单、地图 GeoJSON
│   └── src/
│       ├── views/           # 光河 / 详情 / 记录 / 搜索 / 图集 / 地图 / 设置 / 分享
│       ├── components/      # 记忆节点、相册拼贴、大图查看器、音乐球、巡演菜单 / 退出叉号
│       ├── composables/     # 光河布局引擎、演示状态、主题、Toast
│       ├── stores/          # Pinia
│       └── styles/main.css  # 设计令牌与氛围层
├── deploy/
│   ├── docker.sh            # Docker：构建 / 启动 / 升级 / 状态
│   └── local.sh             # 本地：单进程部署 + 构建
├── docs/                    # 截图与「点亮地图」的调研笔记
├── seed/                    # 演示素材与导入脚本
└── docker-compose.yml
```

## 文档

- [`docs/点亮地图-调研.md`](docs/点亮地图-调研.md) — 地图数据从哪来、省市怎么对上、边界怎么画

---

## 设计说明

界面走的是「暖阳宣纸」的路子：象牙米白的纸面（不是纯白），阳光从上方斜照下来，
记忆像一张张宝丽来照片，被轻轻地贴在一条赭金色的光河上；阴影全部带暖调，
而不是用灰黑色。字体上中文用宋体系统衬线做标题与日期，正文用无衬线，
日期与罗马数字用 Cormorant Garamond。动效一律是慢的、连绵的指数缓动，
并且完整尊重系统的「减弱动态效果」偏好。

---

## 说明与边界

- 目前是**单用户私密使用**的定位（没有账号体系），适合自己部署在自己家里或服务器上。
  数据库结构已经预留了扩展空间，将来要加多用户只需要增加 `user_id` 维度与登录中间件。
- 视频缩略图在**浏览器端**生成，因此后端不依赖 ffmpeg，镜像很小。
- 图片自动识别是**可选**的：不打开 `EFFLUX_TAGGER` 就只有人工标签，其余功能都照常。
- 搜索使用 `pg_trgm` 索引加速 `ILIKE`，对中文短句足够灵敏；如果将来数据量很大，
  可以换成 `zhparser` 全文检索。
- 「点亮地图」用的是自然资源部标准地图的省级/市级边界，仅作**个人记录与观赏**之用。
