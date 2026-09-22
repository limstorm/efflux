-- 记忆发生地点：用于展示、按地点搜索，也为之后的「点亮地图」备好经纬度。
--
-- location_name 是给人看的（"西湖边"），经纬度是给地图用的；两者都可以只有其一，
-- 所以经纬度可空、名字默认空串（免得老的 NULL 判断到处散落）。

ALTER TABLE moments ADD COLUMN IF NOT EXISTS location_name TEXT NOT NULL DEFAULT '';
ALTER TABLE moments ADD COLUMN IF NOT EXISTS latitude DOUBLE PRECISION;
ALTER TABLE moments ADD COLUMN IF NOT EXISTS longitude DOUBLE PRECISION;

-- 按地点筛选
CREATE INDEX IF NOT EXISTS moments_location_idx
    ON moments (location_name) WHERE location_name <> '';

-- 为「点亮地图」准备：只有带经纬度的记忆才会出现在地图上
CREATE INDEX IF NOT EXISTS moments_geo_idx
    ON moments (latitude, longitude) WHERE latitude IS NOT NULL;
