#!/usr/bin/env bash
#
# 流光的 Docker 部署脚本。
#
#   ./deploy/docker.sh up        构建并启动（默认动作）
#   ./deploy/docker.sh status    看容器状态与访问地址
#   ./deploy/docker.sh logs      跟踪日志（Ctrl-C 退出，不影响服务）
#   ./deploy/docker.sh restart   重启
#   ./deploy/docker.sh down      停止（数据留在命名卷里）
#   ./deploy/docker.sh upgrade   拉代码 → 重新构建 → 重启（数据保留）
#   ./deploy/docker.sh destroy   停止并删掉数据（会要一次确认）
#
# 三个容器：web（Nginx，前台） / backend（Rust） / db（PostgreSQL）。
# 数据放在两个命名卷里 —— db_data（数据库）、media_data（照片视频音乐）。
# 容器随便删，卷不动，记忆就还在。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

info() { printf '\033[1;33m→ %s\033[0m\n' "$*"; }
ok()   { printf '\033[1;32m✓ %s\033[0m\n' "$*"; }
die()  { printf '\033[1;31m✗ %s\033[0m\n' "$*" >&2; exit 1; }

# ---------------------------------------------------------------- 环境
command -v docker >/dev/null 2>&1 \
  || die "没找到 docker。装一个再回来：https://docs.docker.com/engine/install/"
if docker compose version >/dev/null 2>&1; then
  DC=(docker compose)
elif command -v docker-compose >/dev/null 2>&1; then
  DC=(docker-compose)
else
  die "没找到 docker compose（v2 的插件或 v1 的 docker-compose 都行）"
fi

ensure_env() {
  if [[ ! -f .env ]]; then
    cp .env.example .env
    info "已按 .env.example 生成 .env"
  fi
}

# 从 .env 里读一个值：env_value KEY [默认值]
env_value() {
  local line
  line="$(grep -E "^${1}=" .env 2>/dev/null | tail -1 || true)"
  if [[ -n "$line" ]]; then printf '%s' "${line#*=}"; else printf '%s' "${2:-}"; fi
}

site_url() {
  local port
  port="$(env_value EFFLUX_HTTP_PORT 8080)"
  printf 'http://localhost:%s' "${port:-8080}"
}

# 等前端把 /api/health 代理通，最多 60 秒
wait_ready() {
  local url="$1/api/health" i
  for i in $(seq 1 30); do
    if curl -fsS --max-time 2 "$url" >/dev/null 2>&1; then return 0; fi
    sleep 2
  done
  return 1
}

warn_open_door() {
  if [[ -z "$(env_value EFFLUX_ACCESS_CODE)" ]]; then
    info "提醒：.env 里的 EFFLUX_ACCESS_CODE 还是空的 —— 谁打开这个网址都能看能改，"
    info "      放到公网前务必设一句口令（或先起服务，再到站点「设置」里改）。"
  fi
}

# ---------------------------------------------------------------- 子命令
cmd_up() {
  ensure_env
  info "构建并启动（首次会久一点：要编译 Rust、下载 CLIP 模型，约十几分钟）"
  "${DC[@]}" up -d --build

  if wait_ready "$(site_url)"; then
    ok "已就绪 → $(site_url)"
  else
    ok "容器起来了，但健康检查还没通过，翻一下日志：./deploy/docker.sh logs"
  fi
  warn_open_door
}

cmd_status() {
  ensure_env
  "${DC[@]}" ps
  printf '\n访问地址：%s\n' "$(site_url)"
  if curl -fsS --max-time 2 "$(site_url)/api/health" >/dev/null 2>&1; then
    ok "服务在跑"
  else
    info "健康检查没通过（服务可能没起来）"
  fi
}

cmd_logs() {
  ensure_env
  info "跟踪日志，Ctrl-C 只是退出看日志，不会停服务"
  "${DC[@]}" logs -f --tail=120
}

cmd_restart() {
  ensure_env
  "${DC[@]}" restart
  ok "已重启 → $(site_url)"
}

cmd_down() {
  ensure_env
  "${DC[@]}" stop
  ok "已停止（数据库与媒体都还在命名卷里，up 一下就回来）"
}

cmd_upgrade() {
  ensure_env
  if [[ -d .git ]]; then
    info "拉取新代码"
    git pull --ff-only
  fi
  info "升级前建议先备份：站点「设置 → 备份」里导一份，或者 docker compose exec db pg_dump -U efflux efflux > backup.sql"
  info "重新构建并重启（数据卷不动）"
  "${DC[@]}" up -d --build
  if wait_ready "$(site_url)"; then
    ok "升级完成 → $(site_url)"
  else
    info "起来了但健康检查没通过，翻日志：./deploy/docker.sh logs"
  fi
}

cmd_destroy() {
  ensure_env
  printf '\033[1;31m这会删掉 db_data 与 media_data 两个卷，所有记忆和照片都没了。\033[0m\n'
  read -r -p "确定？输入 yes 继续：" answer
  [[ "$answer" == "yes" ]] || die "已取消"
  "${DC[@]}" down -v
  ok "已停止并清空数据"
}

usage() {
  cat <<'EOF'
流光的 Docker 部署脚本。三个容器：web（Nginx，前台） / backend（Rust） / db（PostgreSQL）。
数据放在两个命名卷里 —— db_data（数据库）、media_data（照片视频音乐）。

  ./deploy/docker.sh up        构建并启动（默认动作）
  ./deploy/docker.sh status    看容器状态与访问地址
  ./deploy/docker.sh logs      跟踪日志（Ctrl-C 退出，不影响服务）
  ./deploy/docker.sh restart   重启
  ./deploy/docker.sh down      停止（数据留在命名卷里）
  ./deploy/docker.sh upgrade   拉代码 → 重新构建 → 重启（数据保留）
  ./deploy/docker.sh destroy   停止并删掉数据（会要一次确认）
EOF
}

case "${1:-up}" in
  up)        cmd_up ;;
  status|ps) cmd_status ;;
  logs|log)  cmd_logs ;;
  restart)   cmd_restart ;;
  down|stop) cmd_down ;;
  upgrade)   cmd_upgrade ;;
  destroy)   cmd_destroy ;;
  -h|--help|help) usage ;;
  *) die "不认识这个命令：$1（试试 ./deploy/docker.sh help）" ;;
esac
