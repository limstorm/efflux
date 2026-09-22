#!/usr/bin/env bash
#
# 流光的本地部署脚本（不用 Docker）。
#
#   ./deploy/local.sh install   首次：检查依赖、确认数据库、构建前后端、启动
#   ./deploy/local.sh up        启动（没构建过会提示先 install）
#   ./deploy/local.sh status    看状态与访问地址
#   ./deploy/local.sh logs      跟踪日志（Ctrl-C 退出，不影响服务）
#   ./deploy/local.sh restart   重启
#   ./deploy/local.sh down      停止
#   ./deploy/local.sh upgrade   拉代码 → 重新构建 → 重启（数据保留）
#
# 只跑一个后端进程：设上 EFFLUX_STATIC_DIR 之后，前端页面、媒体文件、
# API 都由它发出去，不需要额外装 Nginx。
#
# 配置从 .env 里读（没有就按下面的默认值来），也可以用环境变量临时覆盖：
#   DATABASE_URL             默认 postgres://efflux:efflux_local_password@127.0.0.1:5432/efflux
#   EFFLUX_PORT              默认 8080
#   EFFLUX_DATA_DIR          默认 <仓库>/.data
#   EFFLUX_BUILD_PROFILE     release（默认）或 debug —— 调试时编得快
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

info() { printf '\033[1;33m→ %s\033[0m\n' "$*"; }
ok()   { printf '\033[1;32m✓ %s\033[0m\n' "$*"; }
die()  { printf '\033[1;31m✗ %s\033[0m\n' "$*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "缺少 $1 —— $2"; }

# ---------------------------------------------------------------- 配置
# 从 .env 里读一个值：env_value KEY
env_value() {
  local line
  line="$(grep -E "^${1}=" .env 2>/dev/null | tail -1 || true)"
  printf '%s' "${line#*=}"
}

PROFILE="${EFFLUX_BUILD_PROFILE:-release}"
if [[ "$PROFILE" == "release" ]]; then
  CARGO_ARGS=(--release)
  BIN="$ROOT/backend/target/release/efflux-server"
else
  CARGO_ARGS=()
  BIN="$ROOT/backend/target/debug/efflux-server"
fi

DATABASE_URL="${DATABASE_URL:-$(env_value DATABASE_URL)}"
DATABASE_URL="${DATABASE_URL:-postgres://efflux:efflux_local_password@127.0.0.1:5432/efflux}"

PORT="${EFFLUX_PORT:-$(env_value EFFLUX_PORT)}"
PORT="${PORT:-8080}"

DATA_DIR="${EFFLUX_DATA_DIR:-$(env_value EFFLUX_DATA_DIR)}"
DATA_DIR="${DATA_DIR:-$ROOT/.data}"
case "$DATA_DIR" in
  /*) ;;                                  # 已经是绝对路径
  *) DATA_DIR="$ROOT/$DATA_DIR" ;;        # 相对路径按仓库根来算
esac

STATIC_DIR="$ROOT/frontend/dist"
RUN_DIR="$DATA_DIR/run"
LOG="$RUN_DIR/efflux.log"
PID_FILE="$RUN_DIR/efflux.pid"

site_url() { printf 'http://localhost:%s' "$PORT"; }

# ---------------------------------------------------------------- 小工具
running() {
  [[ -f "$PID_FILE" ]] || return 1
  local pid; pid="$(cat "$PID_FILE" 2>/dev/null || true)"
  [[ -n "$pid" ]] || return 1
  kill -0 "$pid" 2>/dev/null
}

wait_ready() {
  local i
  for i in $(seq 1 30); do
    if curl -fsS --max-time 2 "$(site_url)/api/health" >/dev/null 2>&1; then return 0; fi
    if ! running; then return 1; fi
    sleep 1
  done
  return 1
}

check_deps() {
  need cargo "装一个 Rust 工具链：https://rustup.rs"
  need npm   "装一个 Node.js 20+：https://nodejs.org"
  need curl  "装 curl"
}

# 数据库能连上吗；没装 psql 就没法判断，交给后端自己报错
check_db() {
  if ! command -v psql >/dev/null 2>&1; then
    info "没装 psql，跳过数据库检查（连不上后端会自己说）"
    return 0
  fi
  if psql "$DATABASE_URL" -tAc 'select 1' >/dev/null 2>&1; then
    ok "数据库连得上"
    return 0
  fi

  cat <<EOF

连不上数据库：$DATABASE_URL

先把 PostgreSQL 起起来，再建一个库和用户（照抄即可）：

  sudo -u postgres psql -c "CREATE USER efflux WITH PASSWORD 'efflux_local_password' SUPERUSER;"
  sudo -u postgres psql -c "CREATE DATABASE efflux OWNER efflux;"

想用别的连接串，就在仓库根目录的 .env 里写一行 DATABASE_URL=...，
或者临时：DATABASE_URL=postgres://... $0 install
EOF
  exit 1
}

build() {
  check_deps
  info "构建前端（打包到 frontend/dist）"
  ( cd "$ROOT/frontend" && npm ci --no-audit --no-fund && npm run build )

  info "构建后端（$PROFILE，首次要十来分钟，之后只编改动）"
  ( cd "$ROOT/backend" && cargo build "${CARGO_ARGS[@]}" )

  ok "构建完成"
}

# ---------------------------------------------------------------- 子命令
cmd_install() {
  check_deps
  if [[ ! -f .env ]]; then
    cp .env.example .env
    info "已按 .env.example 生成 .env（要改端口、口令就编辑它）"
  fi
  check_db
  build
  if [[ -z "$(env_value EFFLUX_ACCESS_CODE)" ]]; then
    info "提醒：.env 里的 EFFLUX_ACCESS_CODE 还是空的 —— 谁打开这个网址都能看能改。"
    info "      首次启动会把它写进数据库，之后可以在站点「设置」里随时改。"
  fi
  cmd_up
  cat <<EOF

接下来：
  · 打开 $(site_url)
  · 让它在开机时自己起来（可选）：把下面这段存成 /etc/systemd/system/efflux.service
      [Unit]
      Description=流光 Efflux
      After=network-online.target postgresql.service
      Wants=network-online.target

      [Service]
      User=$USER
      WorkingDirectory=$ROOT
      Environment=EFFLUX_STATIC_DIR=$STATIC_DIR
      Environment=EFFLUX_PORT=$PORT
      Environment=EFFLUX_DATA_DIR=$DATA_DIR
      Environment=DATABASE_URL=$DATABASE_URL
      ExecStart=$BIN
      Restart=on-failure

      [Install]
      WantedBy=multi-user.target
    然后：sudo systemctl daemon-reload && sudo systemctl enable --now efflux
EOF
}

cmd_up() {
  [[ -x "$BIN" ]] || die "还没有后端二进制，先跑 ./deploy/local.sh install"
  [[ -f "$STATIC_DIR/index.html" ]] || die "还没有前端产物，先跑 ./deploy/local.sh install"
  if running; then
    info "已经在跑了（pid $(cat "$PID_FILE")）→ $(site_url)"
    return 0
  fi

  mkdir -p "$RUN_DIR"
  : >"$LOG"
  EFFLUX_STATIC_DIR="$STATIC_DIR" \
  EFFLUX_PORT="$PORT" \
  EFFLUX_DATA_DIR="$DATA_DIR" \
  DATABASE_URL="$DATABASE_URL" \
    nohup "$BIN" >>"$LOG" 2>&1 &
  local pid=$!
  echo "$pid" >"$PID_FILE"
  disown "$pid" 2>/dev/null || true

  if wait_ready; then
    ok "已就绪 → $(site_url)（pid $pid）"
  else
    printf '\n最近几行日志：\n'
    tail -n 15 "$LOG" 2>/dev/null || true
    die "没起来，完整日志：$LOG"
  fi
}

cmd_status() {
  if running; then
    ok "在跑（pid $(cat "$PID_FILE")）→ $(site_url)"
    if curl -fsS --max-time 2 "$(site_url)/api/health" >/dev/null 2>&1; then
      ok "健康检查通过"
    else
      info "健康检查没通过，看看日志：./deploy/local.sh logs"
    fi
  else
    info "没在跑"
  fi
  printf '\n数据目录：%s\n日志：%s\n' "$DATA_DIR" "$LOG"
}

cmd_logs() {
  [[ -f "$LOG" ]] || die "还没有日志（服务没起过？）"
  info "跟踪 $LOG，Ctrl-C 只是退出看日志"
  tail -f -n 120 "$LOG"
}

cmd_down() {
  running || { info "本来就没在跑"; return 0; }
  local pid; pid="$(cat "$PID_FILE")"
  kill "$pid" 2>/dev/null || true
  local i
  for i in $(seq 1 20); do
    kill -0 "$pid" 2>/dev/null || { rm -f "$PID_FILE"; ok "已停止"; return 0; }
    sleep 0.5
  done
  die "等了 10 秒还没停（pid $pid）"
}

cmd_restart() {
  cmd_down
  cmd_up
}

cmd_upgrade() {
  if [[ -d .git ]]; then
    info "拉取新代码"
    git pull --ff-only
  fi
  info "升级前建议先备份：站点「设置 → 备份」里导一份（数据在 $DATA_DIR）"
  build
  cmd_restart
  ok "升级完成 → $(site_url)"
}

usage() {
  cat <<'EOF'
流光的本地部署脚本（不用 Docker）。只跑一个后端进程：
前端页面、媒体文件、API 都由它发出去，不必另装 Nginx。

  ./deploy/local.sh install   首次：检查依赖、确认数据库、构建前后端、启动
  ./deploy/local.sh up        启动（没构建过会提示先 install）
  ./deploy/local.sh status    看状态与访问地址
  ./deploy/local.sh logs      跟踪日志（Ctrl-C 退出，不影响服务）
  ./deploy/local.sh restart   重启
  ./deploy/local.sh down      停止
  ./deploy/local.sh upgrade   拉代码 → 重新构建 → 重启（数据保留）

配置从 .env 里读（没有就用默认值），也可以用环境变量临时覆盖：
  DATABASE_URL           默认 postgres://efflux:efflux_local_password@127.0.0.1:5432/efflux
  EFFLUX_PORT            默认 8080
  EFFLUX_DATA_DIR        默认 <仓库>/.data
  EFFLUX_BUILD_PROFILE   release（默认）或 debug —— 调试时编得快
EOF
}

case "${1:-status}" in
  install)   cmd_install ;;
  up|start)  cmd_up ;;
  status|ps) cmd_status ;;
  logs|log)  cmd_logs ;;
  restart)   cmd_restart ;;
  down|stop) cmd_down ;;
  upgrade)   cmd_upgrade ;;
  -h|--help|help) usage ;;
  *) die "不认识这个命令：$1（试试 ./deploy/local.sh help）" ;;
esac
