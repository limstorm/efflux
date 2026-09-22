-- 图片自动标签：每张图各自打标，供图集检索。
--
-- tag_status 只对图片有意义，所以默认给 skipped（视频、音频不会被扫描）；
-- 图片上传时若开启了识别，才显式置成 pending 等 worker 来取。

ALTER TABLE media ADD COLUMN IF NOT EXISTS tag_status TEXT NOT NULL DEFAULT 'skipped';
ALTER TABLE media ADD COLUMN IF NOT EXISTS tagged_at TIMESTAMPTZ;

CREATE TABLE IF NOT EXISTS media_tags (
    media_id  UUID NOT NULL REFERENCES media(id) ON DELETE CASCADE,
    tag_key   TEXT NOT NULL,
    tag_name  TEXT NOT NULL,
    tag_group TEXT NOT NULL,
    score     REAL NOT NULL,
    PRIMARY KEY (media_id, tag_key)
);

-- 按标签找图（图集搜索的主查询）
CREATE INDEX IF NOT EXISTS media_tags_key_idx ON media_tags (tag_key, score DESC);
CREATE INDEX IF NOT EXISTS media_tags_name_idx ON media_tags (tag_name);
CREATE INDEX IF NOT EXISTS media_tags_media_idx ON media_tags (media_id);

-- worker 取待处理的图
CREATE INDEX IF NOT EXISTS media_pending_tag_idx
    ON media (created_at) WHERE kind = 'image' AND tag_status = 'pending';
