-- 单条记忆的分享凭证。
--
-- 一段记忆一个，128 位密码学随机（32 个 hex 字符），拼进链接里当凭证用。
-- 不用自增 id、不用时间戳：那种值能猜、能枚举，等于把整库的记忆挂在网上。
--
-- 没分享过的记忆这里是 NULL，分享过一次就固定下来。想收回就把它清空，
-- 旧链接立刻作废，下次再分享会拿到一个新值。

ALTER TABLE moments ADD COLUMN IF NOT EXISTS share_token TEXT;

-- 按凭证查是分享页唯一的入口，得走索引；同时保证不会撞车
CREATE UNIQUE INDEX IF NOT EXISTS moments_share_token_idx
    ON moments (share_token) WHERE share_token IS NOT NULL;
