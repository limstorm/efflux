-- 删除留痕。
--
-- 增量备份只打包「自上次以来新增的文件」，而数据快照虽仍是全量，但恢复时
-- 是按主键合并的：光看快照，没法区分「这条被删了」和「这条从没存在过」。
-- 于是删除时在这里落一笔，增量包把它带上，恢复照着重放。
--
-- 只记 id 与时间，不记内容：留痕是为了重放删除，不是为了找回数据。
-- 打完一个基准包（完整快照）后这张表会被清空，历史不再需要。

CREATE TABLE IF NOT EXISTS deletions (
    id UUID NOT NULL,
    table_name TEXT NOT NULL,
    deleted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (id, table_name)
);

-- 打包时按时间取「上次之后删掉的」
CREATE INDEX IF NOT EXISTS deletions_deleted_at_idx ON deletions (deleted_at);
