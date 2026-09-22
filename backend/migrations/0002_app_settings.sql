-- 运行时可改的设置项。
-- 目前用来存访问口令的哈希与令牌签名密钥，这样改口令不必重启服务。

CREATE TABLE IF NOT EXISTS app_settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
