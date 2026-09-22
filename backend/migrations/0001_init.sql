-- 流光 (Efflux) 初始结构
-- 一段「点滴」= moments；附件 = media；标签 = tags

CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE TABLE IF NOT EXISTS moments (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title       TEXT NOT NULL DEFAULT '',
    content     TEXT NOT NULL DEFAULT '',
    -- 用户可自定义的「发生时间」，默认为创建时间
    happened_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS media (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- 允许先上传后关联（创建流程中媒体先到、点滴后到）
    moment_id   UUID REFERENCES moments(id) ON DELETE CASCADE,
    kind        TEXT NOT NULL CHECK (kind IN ('image', 'video', 'audio')),
    role        TEXT NOT NULL DEFAULT 'attachment'
                CHECK (role IN ('attachment', 'music', 'cover')),
    file_path   TEXT NOT NULL,
    thumb_path  TEXT,
    mime_type   TEXT NOT NULL,
    size_bytes  BIGINT NOT NULL DEFAULT 0,
    width       INTEGER,
    height      INTEGER,
    duration_ms INTEGER,
    position    INTEGER NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS tags (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name       TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS moment_tags (
    moment_id UUID NOT NULL REFERENCES moments(id) ON DELETE CASCADE,
    tag_id    UUID NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (moment_id, tag_id)
);

CREATE INDEX IF NOT EXISTS idx_moments_happened_at
    ON moments (happened_at DESC);
CREATE INDEX IF NOT EXISTS idx_moments_created_at
    ON moments (created_at DESC);
CREATE INDEX IF NOT EXISTS idx_moments_title_trgm
    ON moments USING gin (title gin_trgm_ops);
CREATE INDEX IF NOT EXISTS idx_moments_content_trgm
    ON moments USING gin (content gin_trgm_ops);
CREATE INDEX IF NOT EXISTS idx_media_moment
    ON media (moment_id, position);
CREATE INDEX IF NOT EXISTS idx_media_orphans
    ON media (created_at) WHERE moment_id IS NULL;
CREATE INDEX IF NOT EXISTS idx_moment_tags_tag
    ON moment_tags (tag_id);
