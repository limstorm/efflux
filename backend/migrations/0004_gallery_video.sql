-- 视频也进图集：打标用前端抓的首帧缩略图（没有原图可解），索引范围跟着放宽。
-- 顺带把 0003 里只覆盖 image 的那个部分索引重建掉。

DROP INDEX IF EXISTS media_pending_tag_idx;
CREATE INDEX media_pending_tag_idx
    ON media (created_at)
    WHERE kind IN ('image', 'video') AND tag_status = 'pending';
