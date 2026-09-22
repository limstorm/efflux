-- 地点精确到市。
--
-- 定位只能给出经纬度，而「点亮地图」要的是城市这一级：一盏灯代表一座城。
-- city 由后端从坐标推断（内置城市表，离线匹配），也允许用户手填覆盖。

ALTER TABLE moments ADD COLUMN IF NOT EXISTS city TEXT NOT NULL DEFAULT '';

-- 按城市聚合用（地图点亮），也顺带加速按城市筛选
CREATE INDEX IF NOT EXISTS moments_city_idx
    ON moments (city) WHERE city <> '';
