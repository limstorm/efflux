-- 记忆的氛围音。
--
-- 存的是一个名字：rain / night / fire / pad，没有文件、没有字节，
-- 打开这段记忆时按名字现场合成（前端 AudioMood，见 frontend/src/audio/）。
-- 这样"配乐"不再依赖用户手上有音频文件，也不会让备份包膨胀。
--
-- 名字对不上就不写进来（后端只认这四个），空串只在更新时用作"清掉"的信号。
--
-- 没选过的记忆这里是 NULL，保持安静：不追溯。否则几十条旧记忆会在
-- 同一夜之间一起响起来，那不是「默认气氛」，那是惊吓。

ALTER TABLE moments ADD COLUMN IF NOT EXISTS ambient TEXT;
