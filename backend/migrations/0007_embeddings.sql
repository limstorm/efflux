-- 留下 CLIP 向量。
--
-- 打标时本来就算过一遍图像向量，算完就扔了。存下来之后，同一张照片可以拿去
-- 和别的照片比距离，「找相似」就不必重新推理一遍。
--
-- 用 bytea 存 f32 小端，而不是装 pgvector：一来扩展多半得超级用户才装得上，
-- 二来这点规模（几千到几万张）全表读出来在内存里算余弦就够快，不值得为它
-- 多一个部署依赖。dim 单独存一列，是为了将来换模型时能分辨哪些是旧向量。

CREATE TABLE IF NOT EXISTS media_embeddings (
    media_id UUID PRIMARY KEY REFERENCES media(id) ON DELETE CASCADE,
    dim INTEGER NOT NULL,
    model TEXT NOT NULL DEFAULT 'clip',
    -- dim 个 f32，小端，共 dim * 4 字节
    vec BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 回填时用它挑出还没算过的图
CREATE INDEX IF NOT EXISTS media_embeddings_model_idx ON media_embeddings (model);
